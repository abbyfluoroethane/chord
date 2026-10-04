// Copyright (c) 2025 Saarko <saarko@tutanota.com>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Common TLS functionality shared between direct_tls and starttls modules

use core::{error::Error as StdError, fmt};
#[cfg(feature = "ktls")]
use std::os::fd::AsRawFd;
use tokio::io::{AsyncRead, AsyncWrite};

/// Trait alias for async streams that can be used with TLS.
// When the `ktls` feature is enabled, this additionally requires `AsRawFd`.
#[cfg(feature = "ktls")]
pub trait TlsAsyncStream: AsyncRead + AsyncWrite + Unpin + AsRawFd {}
#[cfg(feature = "ktls")]
impl<T: AsyncRead + AsyncWrite + Unpin + AsRawFd> TlsAsyncStream for T {}

/// Trait alias for async streams that can be used with TLS.
#[cfg(not(feature = "ktls"))]
pub trait TlsAsyncStream: AsyncRead + AsyncWrite + Unpin {}
#[cfg(not(feature = "ktls"))]
impl<T: AsyncRead + AsyncWrite + Unpin> TlsAsyncStream for T {}

#[cfg(feature = "native-tls")]
use native_tls::Error as TlsError;
#[cfg(feature = "rustls-any-backend")]
use tokio_rustls::rustls::pki_types::InvalidDnsNameError;
#[cfg(all(feature = "rustls-any-backend", not(feature = "native-tls")))]
use tokio_rustls::rustls::Error as TlsError;

#[cfg(all(feature = "rustls-any-backend", not(feature = "native-tls")))]
use {
    alloc::sync::Arc,
    tokio_rustls::{
        rustls::pki_types::ServerName,
        rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        rustls::pki_types::{CertificateDer, UnixTime},
        rustls::{
            CertificateError, ClientConfig, DigitallySignedStruct, Error as RustlsError,
            OtherError, SignatureScheme,
        },
        TlsConnector,
    },
};

#[cfg(all(
    feature = "rustls-any-backend",
    not(feature = "ktls"),
    not(feature = "native-tls")
))]
pub use tokio_rustls::client::TlsStream;

#[cfg(all(feature = "ktls", not(feature = "native-tls")))]
/// Tls Stream type based on Ktls
pub type TlsStream<S> = ktls::KtlsStream<S>;

#[cfg(feature = "native-tls")]
pub use tokio_native_tls::TlsStream;

#[cfg(feature = "native-tls")]
use {native_tls::TlsConnector as NativeTlsConnector, tokio_native_tls::TlsConnector};

use crate::{connect::ServerConnectorError, error::Error};
use sasl::common::ChannelBinding;

/// CHORD PATCH (SECURITYAUTH-13): a check on the certificate of the server, for a pin.
/// The function gets the DER bytes of the end-entity certificate after the TLS handshake
/// and the normal validation passed. An `Err` stops the connection with a certificate
/// error. Upstream has no such hook.
///
/// The second field is the pinned-leaf exception (CORESESSION-15): a function that says
/// whether the DER bytes are the one certificate that the user chose to trust. Only a
/// certificate that failed the normal validation is tested with it, and only that exact
/// certificate passes. `None` means no exception. The check in the first field also runs
/// inside the verifier, so it sees a certificate that the normal validation refused.
#[derive(Clone)]
pub struct CertCheck(
    pub alloc::sync::Arc<dyn Fn(&[u8]) -> Result<(), String> + Send + Sync>,
    pub Option<alloc::sync::Arc<dyn Fn(&[u8]) -> bool + Send + Sync>>,
);

impl fmt::Debug for CertCheck {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str("CertCheck")
    }
}

/// The text of a failed `CertCheck`, as the error inside the certificate error.
#[derive(Debug)]
struct CertCheckError(String);

impl fmt::Display for CertCheckError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(&self.0)
    }
}

impl StdError for CertCheckError {}

/// CHORD PATCH (CORESESSION-15): the verifier behind a `CertCheck`.
///
/// It runs the normal `WebPkiServerVerifier` first. Every rule of it stays: chain, name,
/// validity. Then:
/// - it gives the DER bytes of the leaf to the check, so the pin sees the certificate even
///   when the normal validation refuses it (the UI shows its fingerprint);
/// - a pin mismatch is an error, with the text of the check;
/// - only when the normal validation failed, the stored fingerprint is set, and the pinned
///   leaf function says that the bytes are that one certificate, the certificate passes.
///   Nothing else passes. There is no switch that turns the validation off.
///
/// The signature checks of the handshake go to the normal verifier, so the server has to
/// hold the key of the pinned certificate.
#[cfg(all(feature = "rustls-any-backend", not(feature = "native-tls")))]
#[derive(Debug)]
struct PinnedVerifier {
    inner: Arc<dyn ServerCertVerifier>,
    check: CertCheck,
}

#[cfg(all(feature = "rustls-any-backend", not(feature = "native-tls")))]
impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        let verdict = (self.check.0)(end_entity.as_ref());
        let normal =
            self.inner
                .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now);
        let pinned_leaf = self.check.1.as_ref().is_some_and(|f| f(end_entity.as_ref()));
        match (normal, verdict) {
            (_, Err(message)) => Err(RustlsError::InvalidCertificate(CertificateError::Other(
                OtherError(Arc::new(CertCheckError(message))),
            ))),
            (Ok(ok), Ok(())) => Ok(ok),
            (Err(_), Ok(())) if pinned_leaf => Ok(ServerCertVerified::assertion()),
            (Err(error), Ok(())) => Err(error),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// Common TLS error type used by both direct_tls and starttls
#[derive(Debug)]
pub enum TlsConnectorError {
    /// TLS error
    Tls(TlsError),
    #[cfg(feature = "rustls-any-backend")]
    /// DNS name parsing error
    DnsNameError(InvalidDnsNameError),
    #[cfg(feature = "ktls")]
    /// Error while setting up kernel TLS
    KtlsError(ktls::Error),
}

impl ServerConnectorError for TlsConnectorError {}

impl fmt::Display for TlsConnectorError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Tls(e) => write!(fmt, "TLS error: {}", e),
            #[cfg(feature = "rustls-any-backend")]
            Self::DnsNameError(e) => write!(fmt, "DNS name error: {}", e),
            #[cfg(feature = "ktls")]
            Self::KtlsError(e) => write!(fmt, "Kernel TLS error: {}", e),
        }
    }
}

impl StdError for TlsConnectorError {}

impl From<TlsError> for TlsConnectorError {
    fn from(e: TlsError) -> Self {
        Self::Tls(e)
    }
}

#[cfg(feature = "rustls-any-backend")]
impl From<InvalidDnsNameError> for TlsConnectorError {
    fn from(e: InvalidDnsNameError) -> Self {
        Self::DnsNameError(e)
    }
}

/// CHORD PATCH: `alpn` is new. The native-tls backend ignores it.
#[cfg(feature = "native-tls")]
pub async fn establish_tls_connection_with_alpn<S: TlsAsyncStream>(
    stream: S,
    domain: &str,
    alpn: &[&[u8]],
) -> Result<(TlsStream<S>, ChannelBinding), Error> {
    establish_tls_connection_with(stream, domain, alpn, None).await
}

/// Establish TLS connection using native-tls
#[cfg(feature = "native-tls")]
pub async fn establish_tls_connection<S: TlsAsyncStream>(
    stream: S,
    domain: &str,
) -> Result<(TlsStream<S>, ChannelBinding), Error> {
    establish_tls_connection_with(stream, domain, &[], None).await
}

/// CHORD PATCH: the one function behind the other two, with a `CertCheck`. The native-tls
/// backend ignores `alpn` and has no check, so a check fails the connection instead of
/// being skipped.
#[cfg(feature = "native-tls")]
pub async fn establish_tls_connection_with<S: TlsAsyncStream>(
    stream: S,
    domain: &str,
    _alpn: &[&[u8]],
    check: Option<&CertCheck>,
) -> Result<(TlsStream<S>, ChannelBinding), Error> {
    if check.is_some() {
        return Err(Error::Io(std::io::Error::other(
            "a certificate check needs the rustls backend",
        )));
    }
    let domain = domain.to_owned();
    let tls_stream = TlsConnector::from(NativeTlsConnector::builder().build().unwrap())
        .connect(&domain, stream)
        .await
        .map_err(|e| TlsConnectorError::Tls(e))?;
    log::warn!(
        "tls-native doesn't support channel binding, please use tls-rust if you want this feature!"
    );
    Ok((tls_stream, ChannelBinding::None))
}

/// CHORD PATCH: the normal certificate verifier. It checks the chain, the name and the dates.
/// On Android it is the system verifier (`rustls-platform-verifier`): `rustls-native-certs`
/// finds no root on Android, because the system keeps them in `/apex/com.android.conscrypt/cacerts`
/// and `/system/etc/security/cacerts`, in a form that the crate does not look for. The app must
/// call the JNI init of `chord-ffi` before the first connection. On the other systems it is
/// `WebPkiServerVerifier` with the roots that the features choose.
#[cfg(all(
    feature = "rustls-any-backend",
    not(feature = "native-tls"),
    target_os = "android"
))]
fn server_verifier() -> Result<Arc<dyn ServerCertVerifier>, Error> {
    let provider = ClientConfig::builder().crypto_provider().clone();
    let verifier = rustls_platform_verifier::Verifier::new(provider).map_err(|e| {
        Error::Io(std::io::Error::other(format!("cannot build the verifier: {e}")))
    })?;
    Ok(Arc::new(verifier))
}

#[cfg(all(
    feature = "rustls-any-backend",
    not(feature = "native-tls"),
    not(target_os = "android")
))]
fn server_verifier() -> Result<Arc<dyn ServerCertVerifier>, Error> {
    use tokio_rustls::rustls::{client::WebPkiServerVerifier, RootCertStore};

    let mut root_store = RootCertStore::empty();

    #[cfg(feature = "webpki-roots")]
    {
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }

    #[cfg(feature = "rustls-native-certs")]
    {
        root_store.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    }

    let verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
        .build()
        .map_err(|e| Error::Io(std::io::Error::other(format!("cannot build the verifier: {e}"))))?;
    Ok(verifier)
}

/// Establish TLS connection using rustls
#[cfg(all(feature = "rustls-any-backend", not(feature = "native-tls")))]
pub async fn establish_tls_connection<S: TlsAsyncStream>(
    stream: S,
    domain: &str,
) -> Result<(TlsStream<S>, ChannelBinding), Error> {
    establish_tls_connection_with(stream, domain, &[], None).await
}

/// CHORD PATCH: `establish_tls_connection` with ALPN protocols. XEP-0368 direct TLS
/// needs the ALPN protocol `xmpp-client`. An empty list sends no ALPN extension.
#[cfg(all(feature = "rustls-any-backend", not(feature = "native-tls")))]
pub async fn establish_tls_connection_with_alpn<S: TlsAsyncStream>(
    stream: S,
    domain: &str,
    alpn: &[&[u8]],
) -> Result<(TlsStream<S>, ChannelBinding), Error> {
    establish_tls_connection_with(stream, domain, alpn, None).await
}

/// CHORD PATCH: the one function behind the other two. The certificate of the server goes
/// through `check` after the handshake. The normal validation runs first.
#[cfg(all(feature = "rustls-any-backend", not(feature = "native-tls")))]
pub async fn establish_tls_connection_with<S: TlsAsyncStream>(
    stream: S,
    domain: &str,
    alpn: &[&[u8]],
    check: Option<&CertCheck>,
) -> Result<(TlsStream<S>, ChannelBinding), Error> {
    let domain =
        ServerName::try_from(domain.to_owned()).map_err(TlsConnectorError::DnsNameError)?;
    let inner = server_verifier()?;

    #[allow(unused_mut, reason = "This config is mutable when using ktls")]
    let mut config = match check {
        // CHORD PATCH: a verifier that wraps the normal one. See `PinnedVerifier`.
        Some(check) => ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedVerifier {
                inner,
                check: check.clone(),
            }))
            .with_no_client_auth(),
        None => ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(inner)
            .with_no_client_auth(),
    };

    config.alpn_protocols = alpn.iter().map(|p| p.to_vec()).collect();

    #[cfg(feature = "ktls")]
    let stream = {
        config.enable_secret_extraction = true;
        ktls::CorkStream::new(stream)
    };

    let tls_stream = TlsConnector::from(Arc::new(config))
        .connect(domain, stream)
        .await
        .map_err(crate::Error::Io)?;

    // CHORD PATCH: the pin check.
    if let Some(check) = check {
        let (_, connection) = tls_stream.get_ref();
        let verdict = match connection.peer_certificates().and_then(|c| c.first()) {
            Some(leaf) => (check.0)(leaf.as_ref()),
            None => Err("the server sent no certificate".to_owned()),
        };
        if let Err(message) = verdict {
            use tokio_rustls::rustls::{CertificateError, Error as RustlsError, OtherError};
            return Err(crate::Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                RustlsError::InvalidCertificate(CertificateError::Other(OtherError(
                    Arc::new(CertCheckError(message)),
                ))),
            )));
        }
    }

    // Extract the channel-binding information before we hand the stream over to ktls.
    let (_, connection) = tls_stream.get_ref();
    let channel_binding = match connection.protocol_version() {
        // TODO: Add support for TLS 1.2 and earlier.
        Some(tokio_rustls::rustls::ProtocolVersion::TLSv1_3) => {
            let data = vec![0u8; 32];
            let data = connection
                .export_keying_material(data, b"EXPORTER-Channel-Binding", None)
                .map_err(TlsConnectorError::Tls)?;
            ChannelBinding::TlsExporter(data)
        }
        _ => ChannelBinding::None,
    };

    #[cfg(feature = "ktls")]
    let tls_stream = ktls::config_ktls_client(tls_stream)
        .await
        .map_err(TlsConnectorError::KtlsError)?;

    Ok((tls_stream, channel_binding))
}
