use super::*;
use crate::slave::{SlaveError, SlaveServer, SlaveTlsConfig, SlaveTransportConfig, Station};
use openssl::asn1::Asn1Time;
use openssl::bn::{BigNum, MsbOption};
use openssl::ec::{EcGroup, EcKey};
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkey::{PKey, Private};
use openssl::rsa::Rsa;
use openssl::sign::Signer;
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode, SslVersion};
use openssl::x509::extension::{
    BasicConstraints, ExtendedKeyUsage, KeyUsage, SubjectAlternativeName,
};
use openssl::x509::X509NameBuilder;
use rustls::internal::msgs::codec::Codec;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct Identity {
    cert: X509,
    key: PKey<Private>,
}

impl Identity {
    fn der(&self) -> CertificateDer<'static> {
        self.cert.to_der().unwrap().into()
    }

    fn private_key(&self) -> PrivateKeyDer<'static> {
        rustls::pki_types::PrivatePkcs8KeyDer::from(self.key.private_key_to_pkcs8().unwrap()).into()
    }
}

struct Fixture {
    ca: Identity,
    other_ca: Identity,
    server: [Identity; 2],
    client: [Identity; 2],
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn rsa_key(bits: u32) -> PKey<Private> {
    PKey::from_rsa(Rsa::generate(bits).unwrap()).unwrap()
}

fn issue(
    key: PKey<Private>,
    issuer: Option<&Identity>,
    version: i32,
    role: &str,
    validity: std::ops::Range<i64>,
) -> Identity {
    issue_with_digest(
        key,
        issuer,
        version,
        role,
        validity,
        MessageDigest::sha256(),
    )
}

fn issue_with_digest(
    key: PKey<Private>,
    issuer: Option<&Identity>,
    version: i32,
    role: &str,
    validity: std::ops::Range<i64>,
    digest: MessageDigest,
) -> Identity {
    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_text("CN", role).unwrap();
    let name = name.build();
    let mut serial = BigNum::new().unwrap();
    serial.rand(128, MsbOption::MAYBE_ZERO, false).unwrap();
    let mut cert = X509::builder().unwrap();
    cert.set_version(version).unwrap();
    cert.set_serial_number(&serial.to_asn1_integer().unwrap())
        .unwrap();
    cert.set_subject_name(&name).unwrap();
    cert.set_issuer_name(issuer.map(|ca| ca.cert.subject_name()).unwrap_or(&name))
        .unwrap();
    cert.set_pubkey(&key).unwrap();
    let now = now_seconds();
    cert.set_not_before(&Asn1Time::from_unix(now + validity.start).unwrap())
        .unwrap();
    cert.set_not_after(&Asn1Time::from_unix(now + validity.end).unwrap())
        .unwrap();
    if version == 2 {
        if issuer.is_none() || role == "intermediate" {
            cert.append_extension(BasicConstraints::new().critical().ca().build().unwrap())
                .unwrap();
            cert.append_extension(KeyUsage::new().key_cert_sign().crl_sign().build().unwrap())
                .unwrap();
        } else {
            cert.append_extension(BasicConstraints::new().critical().build().unwrap())
                .unwrap();
            cert.append_extension(KeyUsage::new().digital_signature().build().unwrap())
                .unwrap();
            let mut usage = ExtendedKeyUsage::new();
            if role == "server" {
                usage.server_auth();
            } else {
                usage.client_auth();
            }
            cert.append_extension(usage.build().unwrap()).unwrap();
            if role == "server" {
                let san = SubjectAlternativeName::new()
                    .ip("127.0.0.1")
                    .build(&cert.x509v3_context(issuer.map(|ca| ca.cert.as_ref()), None))
                    .unwrap();
                cert.append_extension(san).unwrap();
            }
        }
    }
    cert.sign(issuer.map(|ca| &ca.key).unwrap_or(&key), digest)
        .unwrap();
    Identity {
        cert: cert.build(),
        key,
    }
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let ca = issue(rsa_key(2048), None, 2, "CA", -60..3600);
        let other_ca = issue(rsa_key(2048), None, 2, "other CA", -60..3600);
        let key = rsa_key(2048);
        let server = [0, 2].map(|v| issue(key.clone(), Some(&ca), v, "server", -60..3600));
        let client = [0, 2].map(|v| issue(key.clone(), Some(&ca), v, "client", -60..3600));
        Fixture {
            ca,
            other_ca,
            server,
            client,
        }
    })
}

fn verifier() -> ClientVerifier {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(fixture().ca.der()).unwrap();
    let standard = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .unwrap();
    ClientVerifier::new(
        standard,
        &[fixture().ca.der()],
        Arc::new(rustls::crypto::ring::default_provider()),
    )
    .unwrap()
}

fn verify(identity: &Identity, chain: &[CertificateDer<'_>]) -> Result<ClientCertVerified, Error> {
    verifier().verify_client_cert(&identity.der(), chain, UnixTime::now())
}

#[test]
fn identities_accept_v1_and_v3_but_reject_mismatched_keys() {
    let fixture = fixture();
    let provider = rustls::crypto::ring::default_provider();
    for identity in &fixture.server {
        certified_key(vec![identity.der()], identity.private_key(), &provider).unwrap();
        assert!(matches!(
            certified_key(
                vec![identity.der()],
                fixture.other_ca.private_key(),
                &provider
            ),
            Err(Error::InconsistentKeys(
                rustls::InconsistentKeys::KeyMismatch
            ))
        ));
    }
}

#[test]
fn valid_v1_and_v3_clients_require_a_trusted_chain() {
    let fixture = fixture();
    for identity in &fixture.client {
        verify(identity, &[]).unwrap();
    }
    for version in [0, 2] {
        let rogue = issue(
            fixture.client[0].key.clone(),
            Some(&fixture.other_ca),
            version,
            "client",
            -60..3600,
        );
        assert!(verify(&rogue, &[]).is_err());
        // Sending an untrusted root as an intermediate cannot grant trust.
        assert!(verify(&rogue, &[fixture.other_ca.der()]).is_err());
        let expired = issue(
            fixture.client[0].key.clone(),
            Some(&fixture.ca),
            version,
            "client",
            -3600..-60,
        );
        assert!(verify(&expired, &[]).is_err());
        let future = issue(
            fixture.client[0].key.clone(),
            Some(&fixture.ca),
            version,
            "client",
            600..3600,
        );
        assert!(verify(&future, &[]).is_err());
    }
    assert!(verifier().client_auth_mandatory());
    assert!(!verifier().root_hint_subjects().is_empty());
    assert!(
        verify(&fixture.server[1], &[]).is_err(),
        "v3 serverAuth is not clientAuth"
    );
    assert!(
        verify(&fixture.ca, &[]).is_err(),
        "a CA cannot authenticate as a client"
    );
}

#[test]
fn v1_validation_checks_time_signature_key_strength_and_intermediate_constraints() {
    let fixture = fixture();
    let cert = fixture.client[0].der();
    assert!(verifier()
        .verify_client_cert(
            &cert,
            &[],
            UnixTime::since_unix_epoch(Duration::from_secs(1))
        )
        .is_err());
    let mut tampered = cert.as_ref().to_vec();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(verifier()
        .verify_client_cert(&tampered.into(), &[], UnixTime::now())
        .is_err());
    let weak = issue(rsa_key(1024), Some(&fixture.ca), 0, "client", -60..3600);
    assert!(verify(&weak, &[]).is_err());
    let sha1 = issue_with_digest(
        fixture.client[0].key.clone(),
        Some(&fixture.ca),
        0,
        "client",
        -60..3600,
        MessageDigest::sha1(),
    );
    assert!(
        verify(&sha1, &[]).is_err(),
        "v1 compatibility must not enable SHA-1 certificate signatures"
    );
    assert!(verify(&fixture.client[0], &vec![fixture.ca.der(); 9]).is_err());
    let intermediate = issue(
        fixture.other_ca.key.clone(),
        Some(&fixture.ca),
        2,
        "intermediate",
        -60..3600,
    );
    let leaf = issue(
        fixture.client[0].key.clone(),
        Some(&intermediate),
        0,
        "client",
        -60..3600,
    );
    verify(&leaf, &[intermediate.der()]).unwrap();
    assert!(verify(&leaf, &[]).is_err());
    let invalid_issuer = &fixture.client[1];
    let leaf = issue(
        fixture.client[0].key.clone(),
        Some(invalid_issuer),
        0,
        "client",
        -60..3600,
    );
    assert!(verify(&leaf, &[invalid_issuer.der()]).is_err());
}

#[test]
fn malformed_v1_and_v2_never_enter_the_compatibility_path() {
    let fixture = fixture();
    let provider = rustls::crypto::ring::default_provider();
    let mut trailing = fixture.client[0].der().to_vec();
    trailing.push(0);
    let mut with_extensions =
        x509_cert::Certificate::from_der(fixture.client[1].der().as_ref()).unwrap();
    with_extensions.tbs_certificate.version = x509_cert::Version::V1;
    let mut explicit = x509_cert::Certificate::from_der(fixture.client[0].der().as_ref()).unwrap();
    explicit.tbs_certificate.version = x509_cert::Version::V3;
    let mut explicit = explicit.to_der().unwrap();
    let offset = explicit
        .windows(5)
        .position(|bytes| bytes == [0xa0, 3, 2, 1, 2])
        .unwrap();
    explicit[offset + 4] = 0; // DER must omit the DEFAULT v1 version field.
    let v2 = issue(
        fixture.client[0].key.clone(),
        Some(&fixture.ca),
        1,
        "client",
        -60..3600,
    );
    for der in [
        vec![0, 1, 2],
        trailing,
        explicit,
        with_extensions.to_der().unwrap(),
        v2.der().to_vec(),
    ] {
        let cert = CertificateDer::from(der);
        assert!(v1_public_key(&cert).is_none());
        assert!(certified_key(
            vec![cert.clone()],
            fixture.client[0].private_key(),
            &provider
        )
        .is_err());
        assert!(verifier()
            .verify_client_cert(&cert, &[], UnixTime::now())
            .is_err());
    }
}

fn signed(scheme: SignatureScheme, signature: Vec<u8>) -> DigitallySignedStruct {
    let mut encoded = u16::from(scheme).to_be_bytes().to_vec();
    encoded.extend_from_slice(&(signature.len() as u16).to_be_bytes());
    encoded.extend_from_slice(&signature);
    DigitallySignedStruct::read_bytes(&encoded).unwrap()
}

#[test]
fn v1_handshake_signatures_reject_tampering_and_tls13_legacy_schemes() {
    let fixture = fixture();
    let verifier = verifier();
    let provider = rustls::crypto::ring::default_provider();
    let key = provider
        .key_provider
        .load_private_key(fixture.client[0].private_key())
        .unwrap();
    for scheme in [
        SignatureScheme::RSA_PKCS1_SHA256,
        SignatureScheme::RSA_PSS_SHA256,
    ] {
        let signature = key
            .choose_scheme(&[scheme])
            .unwrap()
            .sign(b"handshake")
            .unwrap();
        let dss = signed(scheme, signature.clone());
        verifier
            .verify_tls12_signature(b"handshake", &fixture.client[0].der(), &dss)
            .unwrap();
        assert!(verifier
            .verify_tls12_signature(b"tampered", &fixture.client[0].der(), &dss)
            .is_err());
        assert!(verifier
            .verify_tls12_signature(
                b"handshake",
                &fixture.server[0].der(),
                &signed(scheme, vec![0; signature.len()])
            )
            .is_err());
        let tls13 = verifier.verify_tls13_signature(b"handshake", &fixture.client[0].der(), &dss);
        if scheme == SignatureScheme::RSA_PSS_SHA256 {
            tls13.unwrap();
            assert!(verifier
                .verify_tls13_signature(b"tampered", &fixture.client[0].der(), &dss)
                .is_err());
        } else {
            assert!(
                tls13.is_err(),
                "TLS 1.3 must reject RSA PKCS#1 handshake signatures"
            );
        }
    }
    let wrong_key = provider
        .key_provider
        .load_private_key(fixture.other_ca.private_key())
        .unwrap();
    let dss = signed(
        SignatureScheme::RSA_PSS_SHA256,
        wrong_key
            .choose_scheme(&[SignatureScheme::RSA_PSS_SHA256])
            .unwrap()
            .sign(b"handshake")
            .unwrap(),
    );
    assert!(verifier
        .verify_tls12_signature(b"handshake", &fixture.client[0].der(), &dss)
        .is_err());
    assert!(verifier
        .verify_tls13_signature(b"handshake", &fixture.client[0].der(), &dss)
        .is_err());
    let sha1 = signed(SignatureScheme::RSA_PKCS1_SHA1, vec![0; 256]);
    assert!(verifier
        .verify_tls12_signature(b"handshake", &fixture.client[0].der(), &sha1)
        .is_err());

    let group = EcGroup::from_curve_name(Nid::SECP384R1).unwrap();
    let ec = issue(
        PKey::from_ec_key(EcKey::generate(&group).unwrap()).unwrap(),
        Some(&fixture.ca),
        0,
        "client",
        -60..3600,
    );
    verify(&ec, &[]).unwrap();
    let ec_key = provider
        .key_provider
        .load_private_key(ec.private_key())
        .unwrap();
    let dss = signed(
        SignatureScheme::ECDSA_NISTP384_SHA384,
        ec_key
            .choose_scheme(&[SignatureScheme::ECDSA_NISTP384_SHA384])
            .unwrap()
            .sign(b"handshake")
            .unwrap(),
    );
    verifier
        .verify_tls12_signature(b"handshake", &ec.der(), &dss)
        .unwrap();
    verifier
        .verify_tls13_signature(b"handshake", &ec.der(), &dss)
        .unwrap();
    let mut signer = Signer::new(MessageDigest::sha256(), &ec.key).unwrap();
    signer.update(b"handshake").unwrap();
    let dss = signed(
        SignatureScheme::ECDSA_NISTP256_SHA256,
        signer.sign_to_vec().unwrap(),
    );
    verifier
        .verify_tls12_signature(b"handshake", &ec.der(), &dss)
        .unwrap();
    assert!(
        verifier
            .verify_tls13_signature(b"handshake", &ec.der(), &dss)
            .is_err(),
        "TLS 1.3 must enforce the curve"
    );
}

fn write_identity(dir: &Path, server: &Identity, ca: &Identity, mutual: bool) -> SlaveTlsConfig {
    let cert_file = dir.join("server.crt");
    let key_file = dir.join("server.key");
    let ca_file = dir.join("ca.crt");
    std::fs::write(&cert_file, server.cert.to_pem().unwrap()).unwrap();
    std::fs::write(&key_file, server.key.private_key_to_pem_pkcs8().unwrap()).unwrap();
    std::fs::write(&ca_file, ca.cert.to_pem().unwrap()).unwrap();
    SlaveTlsConfig {
        enabled: true,
        cert_file: cert_file.to_string_lossy().into_owned(),
        key_file: key_file.to_string_lossy().into_owned(),
        ca_file: ca_file.to_string_lossy().into_owned(),
        require_client_cert: mutual,
        ..Default::default()
    }
}

async fn start_slave(tls: SlaveTlsConfig) -> (u16, SlaveServer) {
    for _ in 0..10 {
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let mut slave = SlaveServer::new(SlaveTransportConfig {
            bind_address: "127.0.0.1".into(),
            port,
            tls: tls.clone(),
        });
        slave
            .add_station(Station::with_default_points(
                1,
                "certificate compatibility",
                1,
            ))
            .await
            .unwrap();
        match slave.start().await {
            Ok(()) => return (port, slave),
            Err(SlaveError::BindFailed {
                code: "addr_in_use",
                ..
            }) => continue,
            Err(error) => panic!("slave startup failed: {error}"),
        }
    }
    panic!("could not reserve a test port");
}

fn exchange(
    port: u16,
    ca: &X509,
    client: Option<&Identity>,
    version: SslVersion,
) -> Result<(), String> {
    let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
    builder.set_verify(SslVerifyMode::PEER);
    builder.cert_store_mut().add_cert(ca.clone()).unwrap();
    builder.set_min_proto_version(Some(version)).unwrap();
    builder.set_max_proto_version(Some(version)).unwrap();
    if let Some(client) = client {
        builder.set_certificate(&client.cert).unwrap();
        builder.set_private_key(&client.key).unwrap();
    }
    let mut config = builder.build().configure().unwrap();
    // Match the application's industrial-device policy: verify CA/time, but
    // do not require a v1 certificate (which has no SAN) to name the socket IP.
    config.set_verify_hostname(false);
    let socket = std::net::TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_secs(5),
    )
    .unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut tls = config
        .connect("127.0.0.1", socket)
        .map_err(|e| e.to_string())?;
    tls.write_all(&[0x68, 4, 7, 0, 0, 0])
        .map_err(|e| e.to_string())?;
    let mut response = [0; 6];
    tls.read_exact(&mut response).map_err(|e| e.to_string())?;
    if response != [0x68, 4, 0x0b, 0, 0, 0] {
        return Err(format!("unexpected STARTDT response: {response:?}"));
    }
    // Application data is essential: TLS 1.3 can reject the client's certificate
    // after the client-side connect call has already returned successfully.
    tls.write_all(&[0x68, 4, 0x43, 0, 0, 0])
        .map_err(|e| e.to_string())?;
    tls.read_exact(&mut response).map_err(|e| e.to_string())?;
    if response != [0x68, 4, 0x83, 0, 0, 0] {
        return Err(format!("unexpected TESTFR response: {response:?}"));
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_slave_accepts_v1_v3_and_mixed_mtls_over_tls12_and_tls13() {
    let fixture = fixture();
    for version in [SslVersion::TLS1_2, SslVersion::TLS1_3] {
        for server in &fixture.server {
            let dir = tempfile::tempdir().unwrap();
            let (port, mut slave) =
                start_slave(write_identity(dir.path(), server, &fixture.ca, true)).await;
            let result = tokio::task::spawn_blocking(move || {
                for client in &fixture.client {
                    exchange(port, &fixture.ca.cert, Some(client), version).unwrap();
                }
                assert!(exchange(port, &fixture.ca.cert, None, version).is_err());
                let rogue = issue(
                    fixture.client[0].key.clone(),
                    Some(&fixture.other_ca),
                    0,
                    "client",
                    -60..3600,
                );
                assert!(exchange(port, &fixture.ca.cert, Some(&rogue), version).is_err());
                let expired = issue(
                    fixture.client[0].key.clone(),
                    Some(&fixture.ca),
                    0,
                    "client",
                    -3600..-60,
                );
                assert!(exchange(port, &fixture.ca.cert, Some(&expired), version).is_err());
            })
            .await;
            slave.stop().await.unwrap();
            result.unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_slave_accepts_v1_and_v3_without_client_auth() {
    let fixture = fixture();
    for server in &fixture.server {
        let dir = tempfile::tempdir().unwrap();
        let (port, mut slave) =
            start_slave(write_identity(dir.path(), server, &fixture.ca, false)).await;
        let result = tokio::task::spawn_blocking(move || {
            for version in [SslVersion::TLS1_2, SslVersion::TLS1_3] {
                exchange(port, &fixture.ca.cert, None, version).unwrap();
            }
        })
        .await;
        slave.stop().await.unwrap();
        result.unwrap();
    }
}

async fn master_roundtrip(port: u16, tls: crate::master::TlsConfig) -> Result<(), String> {
    use crate::master::{MasterConfig, MasterConnection};
    let mut master = MasterConnection::new(MasterConfig {
        target_address: "127.0.0.1".into(),
        port,
        tls,
        ..Default::default()
    });
    master.connect().await.map_err(|e| e.to_string())?;
    let result = async {
        master
            .send_interrogation(1)
            .await
            .map_err(|e| e.to_string())?;
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if master
                    .received_data
                    .read()
                    .await
                    .ca_map(1)
                    .is_some_and(|points| !points.is_empty())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| "master did not receive GI data".to_string())
    }
    .await;
    let disconnected = master.disconnect().await.map_err(|e| e.to_string());
    result.and(disconnected)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn application_master_supports_v1_v3_mixed_pem_mtls_and_rejects_invalid_servers() {
    use crate::master::{TlsConfig, TlsVersionPolicy};
    let fixture = fixture();
    for server in &fixture.server {
        let dir = tempfile::tempdir().unwrap();
        let tls = write_identity(dir.path(), server, &fixture.ca, true);
        let ca_file = tls.ca_file.clone();
        let (port, mut slave) = start_slave(tls).await;
        let result = async {
            for client in &fixture.client {
                let cert_file = dir.path().join("client.crt");
                let key_file = dir.path().join("client.key");
                std::fs::write(&cert_file, client.cert.to_pem().unwrap()).unwrap();
                // Exercise the field-issued PKCS#1 key path too.
                std::fs::write(
                    &key_file,
                    client.key.rsa().unwrap().private_key_to_pem().unwrap(),
                )
                .unwrap();
                for version in [TlsVersionPolicy::Tls12Only, TlsVersionPolicy::Tls13Only] {
                    master_roundtrip(
                        port,
                        TlsConfig {
                            enabled: true,
                            ca_file: ca_file.clone(),
                            cert_file: format!("\"{}\"", cert_file.display()),
                            key_file: format!("\"{}\"", key_file.display()),
                            version,
                            ..Default::default()
                        },
                    )
                    .await?;
                }
            }
            Ok::<_, String>(())
        }
        .await;
        slave.stop().await.unwrap();
        result.unwrap();
    }
    let expired = issue(
        fixture.server[0].key.clone(),
        Some(&fixture.ca),
        0,
        "server",
        -3600..-60,
    );
    let untrusted = issue(
        fixture.server[0].key.clone(),
        Some(&fixture.other_ca),
        0,
        "server",
        -60..3600,
    );
    for server in [&expired, &untrusted, &fixture.client[1]] {
        let dir = tempfile::tempdir().unwrap();
        let tls = write_identity(dir.path(), server, &fixture.ca, false);
        let ca_file = tls.ca_file.clone();
        let (port, mut slave) = start_slave(tls).await;
        let result = master_roundtrip(
            port,
            TlsConfig {
                enabled: true,
                ca_file,
                ..Default::default()
            },
        )
        .await;
        slave.stop().await.unwrap();
        assert!(
            result.is_err(),
            "master must reject expired, untrusted or wrong-purpose certificates"
        );
    }
}

/// Optional local regression check; no private certificate fixtures belong in
/// the repository. Reads existing PEM files without copying or modifying them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "set IEC104_TLS_CERT_DIR to an existing ca.crt/server.crt/server.key/client.crt/client.key directory"]
async fn configured_certificate_directory() {
    let dir = std::path::PathBuf::from(
        std::env::var_os("IEC104_TLS_CERT_DIR").expect("IEC104_TLS_CERT_DIR is required"),
    );
    let ca = X509::from_pem(&std::fs::read(dir.join("ca.crt")).unwrap()).unwrap();
    let client = Identity {
        cert: X509::from_pem(&std::fs::read(dir.join("client.crt")).unwrap()).unwrap(),
        key: PKey::private_key_from_pem(&std::fs::read(dir.join("client.key")).unwrap()).unwrap(),
    };
    let tls = SlaveTlsConfig {
        enabled: true,
        cert_file: dir.join("server.crt").to_string_lossy().into_owned(),
        key_file: dir.join("server.key").to_string_lossy().into_owned(),
        ca_file: dir.join("ca.crt").to_string_lossy().into_owned(),
        require_client_cert: true,
        ..Default::default()
    };
    let (port, mut slave) = start_slave(tls).await;
    let result = tokio::task::spawn_blocking(move || {
        for version in [SslVersion::TLS1_2, SslVersion::TLS1_3] {
            exchange(port, &ca, Some(&client), version).unwrap();
            assert!(exchange(port, &ca, None, version).is_err());
        }
    })
    .await;
    let master_result = async {
        for version in [
            crate::master::TlsVersionPolicy::Tls12Only,
            crate::master::TlsVersionPolicy::Tls13Only,
        ] {
            master_roundtrip(
                port,
                crate::master::TlsConfig {
                    enabled: true,
                    ca_file: dir.join("ca.crt").to_string_lossy().into_owned(),
                    cert_file: dir.join("client.crt").to_string_lossy().into_owned(),
                    key_file: dir.join("client.key").to_string_lossy().into_owned(),
                    version,
                    ..Default::default()
                },
            )
            .await?;
        }
        Ok::<_, String>(())
    }
    .await;
    slave.stop().await.unwrap();
    result.unwrap();
    master_result.unwrap();
}
