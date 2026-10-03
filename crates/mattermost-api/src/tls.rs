//! Opting out of certificate verification.
//!
//! Set `MM_INSECURE_TLS=1` to accept any server certificate — for a server
//! behind an internal CA whose root you do not want to install. This disables
//! the protection that stops a network attacker from impersonating the server
//! and reading your session token, so it is deliberately an environment
//! variable and not a setting in the UI: it has to be a decision someone made
//! on purpose, for one run, not a switch flipped past in a dialog.
//!
//! REST and the websocket build TLS through separate stacks (reqwest and
//! tokio-tungstenite), so both consult this — turning it on in one place only
//! would leave the other refusing to connect.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

/// Installs the process-wide rustls crypto provider, once.
///
/// Anything that reaches rustls through its *default* provider rather than an
/// explicit one needs this: `webrtc`'s DTLS stack does exactly that, and
/// without it the handshake thread panics with "could not automatically
/// determine the process-level CryptoProvider" — on a background thread, so
/// the call just never connects and the data channel never opens.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Whether certificate verification is disabled for this process.
pub fn insecure() -> bool {
    static INSECURE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *INSECURE.get_or_init(|| {
        matches!(
            std::env::var("MM_INSECURE_TLS").as_deref(),
            Ok("1") | Ok("true")
        )
    })
}

/// The connector for [`tokio_tungstenite::connect_async_tls_with_config`].
/// `None` means the default — verify normally.
pub(crate) fn ws_connector() -> Option<tokio_tungstenite::Connector> {
    if !insecure() {
        return system_connector();
    }
    tracing::warn!("MM_INSECURE_TLS: websocket certificate verification disabled");
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .ok()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerify(provider)))
        .with_no_client_auth();
    Some(tokio_tungstenite::Connector::Rustls(Arc::new(config)))
}

/// What verifies normally. Everywhere but Android that is the websocket
/// library's own default, which reads the system's root certificates.
#[cfg(not(target_os = "android"))]
fn system_connector() -> Option<tokio_tungstenite::Connector> {
    None
}

/// Android keeps no such file for the default to read, so it would trust
/// nobody and never connect. The system is asked instead, as REST asks it.
#[cfg(target_os = "android")]
fn system_connector() -> Option<tokio_tungstenite::Connector> {
    use rustls_platform_verifier::BuilderVerifierExt;

    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(|builder| builder.with_platform_verifier());
    match config {
        Ok(config) => Some(tokio_tungstenite::Connector::Rustls(Arc::new(
            config.with_no_client_auth(),
        ))),
        Err(error) => {
            tracing::error!(%error, "no system certificate verifier for the websocket");
            None
        }
    }
}

/// Accepts every certificate chain. Signatures are still checked against the
/// key in the presented leaf — that costs nothing and keeps the handshake
/// itself honest; it is the *identity* behind that key we are choosing not to
/// establish.
#[derive(Debug)]
struct NoVerify(Arc<CryptoProvider>);

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
