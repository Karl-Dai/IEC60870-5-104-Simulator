//! X.509 v1 end-entity compatibility without weakening the normal v3 path.
//!
//! rustls/webpki only parses v3 certificates. For a strictly decoded v1 leaf,
//! validate the chain with OpenSSL and feed its public key to rustls' existing
//! signature algorithms. Never turn a certificate or signature error into an
//! unconditional acceptance, and never rewrite the signed certificate bytes.

use openssl::x509::{
    store::X509StoreBuilder, verify::X509VerifyParam, X509PurposeId, X509StoreContext, X509,
};
use rustls::client::danger::HandshakeSignatureValid;
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, SubjectPublicKeyInfoDer, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::sign::CertifiedKey;
use rustls::{CertificateError, DigitallySignedStruct, DistinguishedName, Error, SignatureScheme};
use std::sync::Arc;
use x509_cert::der::{Decode, Encode};

pub(crate) mod master;

/// Only genuine v1 leaves enter the compatibility path. v2, malformed DER,
/// trailing data, or extensions/unique IDs incorrectly placed in v1 do not.
fn v1_public_key(cert: &CertificateDer<'_>) -> Option<SubjectPublicKeyInfoDer<'static>> {
    let parsed = x509_cert::Certificate::from_der(cert.as_ref()).ok()?;
    let tbs = &parsed.tbs_certificate;
    if tbs.version != x509_cert::Version::V1
        || tbs.extensions.is_some()
        || tbs.issuer_unique_id.is_some()
        || tbs.subject_unique_id.is_some()
    {
        return None;
    }
    if parsed.to_der().ok()?.as_slice() != cert.as_ref() {
        return None;
    }
    Some(tbs.subject_public_key_info.to_der().ok()?.into())
}

pub(crate) fn certified_key(
    certificates: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
    provider: &CryptoProvider,
) -> Result<CertifiedKey, Error> {
    let leaf = certificates.first().ok_or(Error::NoCertificatesPresented)?;
    let Some(cert_spki) = v1_public_key(leaf) else {
        return CertifiedKey::from_der(certificates, private_key, provider);
    };

    let key = provider.key_provider.load_private_key(private_key)?;
    let key_spki = key
        .public_key()
        .ok_or(Error::InconsistentKeys(rustls::InconsistentKeys::Unknown))?;
    if key_spki != cert_spki {
        return Err(Error::InconsistentKeys(
            rustls::InconsistentKeys::KeyMismatch,
        ));
    }
    Ok(CertifiedKey::new(certificates, key))
}

#[derive(Debug)]
pub(crate) struct ClientVerifier {
    standard: Arc<dyn ClientCertVerifier>,
    roots: Vec<X509>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ClientVerifier {
    pub(crate) fn new(
        standard: Arc<dyn ClientCertVerifier>,
        roots: &[CertificateDer<'_>],
        provider: Arc<CryptoProvider>,
    ) -> Result<Self, Error> {
        let roots = roots
            .iter()
            .map(parse_certificate)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            standard,
            roots,
            algorithms: provider.signature_verification_algorithms,
        })
    }

    fn verify_v1_chain(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        // Bound untrusted path-building input. Trust comes exclusively from the
        // configured CA file, never the OS trust store or peer-supplied roots.
        const MAX_INTERMEDIATES: usize = 8;
        if intermediates.len() > MAX_INTERMEDIATES {
            return Err(Error::General("客户端证书链过长".into()));
        }
        let leaf = parse_certificate(end_entity)?;
        let mut chain = openssl::stack::Stack::new().map_err(openssl_error)?;
        for cert in intermediates {
            chain
                .push(parse_certificate(cert)?)
                .map_err(openssl_error)?;
        }
        let mut store = X509StoreBuilder::new().map_err(openssl_error)?;
        for root in &self.roots {
            store.add_cert(root.clone()).map_err(openssl_error)?;
        }
        let mut params = X509VerifyParam::new().map_err(openssl_error)?;
        params
            .set_purpose(X509PurposeId::SSL_CLIENT)
            .map_err(openssl_error)?;
        // 112-bit security: reject SHA-1 signatures and RSA keys below 2048 bits.
        params.set_auth_level(2);
        params.set_depth(MAX_INTERMEDIATES as i32);
        params.set_time(
            now.as_secs()
                .try_into()
                .map_err(|_| Error::FailedToGetCurrentTime)?,
        );
        store.set_param(&params).map_err(openssl_error)?;
        let store = store.build();
        let mut context = X509StoreContext::new().map_err(openssl_error)?;
        let (valid, error) = context
            .init(&store, &leaf, &chain, |ctx| {
                let valid = ctx.verify_cert()?;
                Ok((valid, ctx.error()))
            })
            .map_err(openssl_error)?;
        if !valid {
            return Err(Error::General(format!(
                "X.509 v1 客户端证书校验失败: {error}"
            )));
        }
        Ok(ClientCertVerified::assertion())
    }
}

impl ClientCertVerifier for ClientVerifier {
    fn offer_client_auth(&self) -> bool {
        self.standard.offer_client_auth()
    }

    fn client_auth_mandatory(&self) -> bool {
        self.standard.client_auth_mandatory()
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        self.standard.root_hint_subjects()
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        if v1_public_key(end_entity).is_some() {
            self.verify_v1_chain(end_entity, intermediates, now)
        } else {
            self.standard
                .verify_client_cert(end_entity, intermediates, now)
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        let Some(spki) = v1_public_key(cert) else {
            return self.standard.verify_tls12_signature(message, cert, dss);
        };
        let algorithms = self
            .algorithms
            .mapping
            .iter()
            .find(|(scheme, _)| *scheme == dss.scheme)
            .map(|(_, algorithms)| *algorithms)
            .ok_or(rustls::PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme)?;
        let key = webpki::RawPublicKeyEntity::try_from(&spki)
            .map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))?;
        // TLS 1.2 permits multiple curves for an ECDSA hash scheme; use the
        // provider's complete mapping, just like rustls' certificate verifier.
        for algorithm in algorithms {
            if key
                .verify_signature(*algorithm, message, dss.signature())
                .is_ok()
            {
                return Ok(HandshakeSignatureValid::assertion());
            }
        }
        Err(Error::InvalidCertificate(CertificateError::BadSignature))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        let Some(spki) = v1_public_key(cert) else {
            return self.standard.verify_tls13_signature(message, cert, dss);
        };
        // This helper enforces TLS 1.3's scheme/curve restrictions as well as
        // the signature. Using the public key here does not enable RFC 7250.
        rustls::crypto::verify_tls13_signature_with_raw_key(message, &spki, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.standard.supported_verify_schemes()
    }
}

fn parse_certificate(cert: &CertificateDer<'_>) -> Result<X509, Error> {
    let parsed = X509::from_der(cert.as_ref())
        .map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))?;
    // OpenSSL's d2i API can ignore trailing input; do not accept that here.
    if parsed.to_der().map_err(openssl_error)?.as_slice() != cert.as_ref() {
        return Err(Error::InvalidCertificate(CertificateError::BadEncoding));
    }
    Ok(parsed)
}

fn openssl_error(error: openssl::error::ErrorStack) -> Error {
    Error::General(format!("X.509 证书处理失败: {error}"))
}

#[cfg(test)]
mod tests;
