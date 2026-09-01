//! Master TLS for explicitly configured device CAs. The platform TLS backend
//! can reject v1 certificates before the application's policy can inspect them.

use crate::master::{MasterError, TlsConfig, TlsVersionPolicy};
use openssl::pkey::PKey;
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode, SslVersion};
use openssl::x509::X509;
use std::io::{Read, Write};
use std::net::TcpStream;

pub(crate) enum TlsStream {
    Native(native_tls::TlsStream<TcpStream>),
    CustomCa(openssl::ssl::SslStream<TcpStream>),
}

impl TlsStream {
    pub(crate) fn get_ref(&self) -> &TcpStream {
        match self {
            Self::Native(stream) => stream.get_ref(),
            Self::CustomCa(stream) => stream.get_ref(),
        }
    }
}

impl Read for TlsStream {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Native(stream) => stream.read(bytes),
            Self::CustomCa(stream) => stream.read(bytes),
        }
    }
}

impl Write for TlsStream {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Native(stream) => stream.write(bytes),
            Self::CustomCa(stream) => stream.write(bytes),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Native(stream) => stream.flush(),
            Self::CustomCa(stream) => stream.flush(),
        }
    }
}

pub(crate) fn connect(
    cfg: &TlsConfig,
    domain: &str,
    socket: TcpStream,
) -> Result<TlsStream, MasterError> {
    let mut builder = SslConnector::builder(SslMethod::tls()).map_err(tls_error)?;
    let (min, max) = match cfg.version {
        TlsVersionPolicy::Auto => (SslVersion::TLS1_2, None),
        TlsVersionPolicy::Tls12Only => (SslVersion::TLS1_2, Some(SslVersion::TLS1_2)),
        TlsVersionPolicy::Tls13Only => (SslVersion::TLS1_3, Some(SslVersion::TLS1_3)),
    };
    builder
        .set_min_proto_version(Some(min))
        .map_err(tls_error)?;
    builder.set_max_proto_version(max).map_err(tls_error)?;
    builder.set_verify(if cfg.accept_invalid_certs {
        SslVerifyMode::NONE
    } else {
        SslVerifyMode::PEER
    });
    builder.set_security_level(2);

    let roots = X509::stack_from_pem(&read_file(&cfg.ca_file, "CA 证书")?).map_err(tls_error)?;
    if roots.is_empty() {
        return Err(MasterError::TlsError("CA 文件中没有 PEM 证书".into()));
    }
    for root in roots {
        builder.cert_store_mut().add_cert(root).map_err(tls_error)?;
    }
    // native-tls adds the configured CA to system trust. Preserve that behavior
    // instead of relying on a vendored OpenSSL installation's default CA paths.
    let native_roots = rustls_native_certs::load_native_certs();
    if !native_roots.errors.is_empty() {
        log::warn!("部分系统 CA 加载失败，仍使用可用的系统 CA 和配置的 CA");
    }
    for root in native_roots.certs {
        let root = X509::from_der(root.as_ref()).map_err(tls_error)?;
        builder.cert_store_mut().add_cert(root).map_err(tls_error)?;
    }

    if !cfg.cert_file.is_empty() && !cfg.key_file.is_empty() {
        let mut chain = X509::stack_from_pem(&read_file(&cfg.cert_file, "客户端证书")?)
            .map_err(tls_error)?
            .into_iter();
        let cert = chain
            .next()
            .ok_or_else(|| MasterError::TlsError("客户端文件中没有 PEM 证书".into()))?;
        let key =
            crate::tls_key::load_key_as_pkcs8_pem(&cfg.key_file).map_err(MasterError::TlsError)?;
        let key = PKey::private_key_from_pem(&key).map_err(tls_error)?;
        builder.set_certificate(&cert).map_err(tls_error)?;
        builder.set_private_key(&key).map_err(tls_error)?;
        for cert in chain {
            builder.add_extra_chain_cert(cert).map_err(tls_error)?;
        }
        builder.check_private_key().map_err(tls_error)?;
    }

    let mut connector = builder.build().configure().map_err(tls_error)?;
    // Preserve the existing device policy: CNs often name a serial number and
    // v1 has no SAN. CA, validity, purpose, and handshake signatures remain checked.
    connector.set_verify_hostname(false);
    connector
        .connect(domain, socket)
        .map(TlsStream::CustomCa)
        .map_err(|error| MasterError::TlsError(format!("TLS 握手失败: {error}")))
}

fn read_file(path: &str, label: &str) -> Result<Vec<u8>, MasterError> {
    let path = crate::tls_key::sanitize_fs_path(path);
    std::fs::read(path)
        .map_err(|error| MasterError::TlsError(format!("读取{label}失败 {path}: {error}")))
}

fn tls_error(error: openssl::error::ErrorStack) -> MasterError {
    MasterError::TlsError(format!("TLS 证书或配置错误: {error}"))
}
