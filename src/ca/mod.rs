//! Aquila's role as the certificate authority for Apus: Taurus's server
//! certificate and each action's short-lived client certificate (its URI
//! SAN, built by [`identity`], is the action's identity that Taurus checks
//! against the Apus handshake's `HELLO.action`).
//!
//! Today this is faked by a shell script in the `apus` repo
//! (`bench/tls.sh`); this module is Aquila actually playing the role. See
//! `apus/README.md`'s "Authentication" section for the bigger picture.
//!
//! [`CertificateAuthority::load`] either loads a long-lived CA certificate
//! and key from disk, or - only when no path is configured *and* Aquila
//! believes it's running in [`Environment::Development`] - generates a
//! throwaway, process-lifetime CA. Staging/production with no CA configured
//! is a hard startup error: silently falling back to an ephemeral CA there
//! would issue certificates nothing else trusts, or worse, would be a
//! different CA on every restart.

pub mod identity;

use std::time::Duration;

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, Ia5String, IsCa, KeyPair, KeyUsagePurpose, SanType,
};
use time::OffsetDateTime;

use crate::configuration::env::Environment;

/// One issued certificate: the leaf, its private key, and the CA certificate
/// it chains to (bundled in so a caller never needs a separate trust fetch).
#[derive(Clone)]
pub struct IssuedCertificate {
    pub certificate_pem: String,
    pub private_key_pem: String,
    pub ca_pem: String,
    /// Unix seconds after which the certificate is no longer valid.
    pub not_after_unix: i64,
}

impl std::fmt::Debug for IssuedCertificate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedCertificate")
            .field("certificate_pem", &self.certificate_pem)
            .field("private_key_pem", &"[FILTERED]")
            .field("ca_pem", &self.ca_pem)
            .field("not_after_unix", &self.not_after_unix)
            .finish()
    }
}

/// What can go wrong issuing or loading certificates.
#[derive(Debug)]
pub enum CaError {
    /// The current environment requires a configured CA certificate/key
    /// (or both/neither were given) but the configuration didn't provide it.
    MissingConfiguration(String),
    /// A CA certificate/key file couldn't be read.
    Io {
        path: String,
        source: std::io::Error,
    },
    /// `rcgen` rejected a parameter or signing operation.
    Rcgen(String),
    /// The action identifier isn't one Taurus's identity parser would accept.
    InvalidActionIdentifier(String),
    /// A server certificate was requested with no endpoint names (DNS/IP) to put in its SAN.
    NoEndpointNames,
}

impl std::fmt::Display for CaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaError::MissingConfiguration(msg) => write!(f, "apus ca: {msg}"),
            CaError::Io { path, source } => write!(f, "apus ca: couldn't read {path}: {source}"),
            CaError::Rcgen(msg) => write!(f, "apus ca: {msg}"),
            CaError::InvalidActionIdentifier(action) => {
                write!(
                    f,
                    "apus ca: invalid action identifier for a certificate: {action:?}"
                )
            }
            CaError::NoEndpointNames => {
                write!(
                    f,
                    "apus ca: a server certificate needs at least one endpoint name"
                )
            }
        }
    }
}

impl std::error::Error for CaError {}

fn rcgen_err(error: rcgen::Error) -> CaError {
    CaError::Rcgen(error.to_string())
}

/// Aquila's certificate authority: holds the CA's key pair (to sign leaf
/// certificates) and the CA certificate's PEM (handed out alongside every
/// issued leaf so the receiver can build its trust store).
pub struct CertificateAuthority {
    issuer_cert: Certificate,
    issuer_key: KeyPair,
    ca_pem: String,
}

impl CertificateAuthority {
    /// Loads (or, in development, generates) the certificate authority.
    ///
    /// - Both `ca_cert_path` and `ca_key_path` set: loads the CA from disk.
    /// - Neither set, in [`Environment::Development`]: generates an
    ///   ephemeral, self-signed, process-lifetime CA (loudly logged).
    /// - Neither set, in staging/production: hard error - never issue
    ///   certificates from a throwaway CA outside development.
    /// - Exactly one of the two set: always a configuration error.
    pub fn load(
        environment: Environment,
        ca_cert_path: Option<&str>,
        ca_key_path: Option<&str>,
    ) -> Result<Self, CaError> {
        match (ca_cert_path, ca_key_path) {
            (Some(cert_path), Some(key_path)) => Self::from_files(cert_path, key_path),
            (None, None) => {
                if environment == Environment::Development {
                    log::warn!(
                        "Apus CA: no ca_cert_path/ca_key_path configured - generating an \
                         EPHEMERAL, process-lifetime self-signed CA. Certificates issued with it \
                         are worthless outside this process and this must never happen outside \
                         development."
                    );
                    Self::ephemeral()
                } else {
                    Err(CaError::MissingConfiguration(format!(
                        "apus.ca_cert_path and apus.ca_key_path must be configured in the {environment} environment"
                    )))
                }
            }
            _ => Err(CaError::MissingConfiguration(
                "apus.ca_cert_path and apus.ca_key_path must both be set, or neither".into(),
            )),
        }
    }

    /// Generates a throwaway, process-lifetime self-signed CA. Development only.
    fn ephemeral() -> Result<Self, CaError> {
        let key = KeyPair::generate().map_err(rcgen_err)?;
        let mut params = CertificateParams::new(Vec::<String>::new()).map_err(rcgen_err)?;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "aquila apus ephemeral ca");
        params.distinguished_name = dn;

        let cert = params.self_signed(&key).map_err(rcgen_err)?;
        let ca_pem = cert.pem();
        Ok(Self {
            issuer_cert: cert,
            issuer_key: key,
            ca_pem,
        })
    }

    /// Loads an existing CA certificate/key pair from disk. The returned
    /// `ca_pem` is exactly the file's contents - not re-derived - so every
    /// certificate chains to precisely what was provisioned.
    fn from_files(cert_path: &str, key_path: &str) -> Result<Self, CaError> {
        let ca_pem = std::fs::read_to_string(cert_path).map_err(|source| CaError::Io {
            path: cert_path.to_string(),
            source,
        })?;
        let key_pem = std::fs::read_to_string(key_path).map_err(|source| CaError::Io {
            path: key_path.to_string(),
            source,
        })?;

        let key = KeyPair::from_pem(&key_pem).map_err(rcgen_err)?;
        // Only extracts what's needed to sign with this CA (subject,
        // key usages, key identifier method) - doesn't re-derive the
        // certificate bytes handed out as `ca_pem` above.
        let params = CertificateParams::from_ca_cert_pem(&ca_pem).map_err(rcgen_err)?;
        let cert = params.self_signed(&key).map_err(rcgen_err)?;

        Ok(Self {
            issuer_cert: cert,
            issuer_key: key,
            ca_pem,
        })
    }

    /// The CA certificate's PEM, e.g. to write out for the interim file bridge.
    pub fn ca_pem(&self) -> &str {
        &self.ca_pem
    }

    /// Issues a short-lived client certificate identifying one action: its
    /// URI SAN (`code0://action/<identifier>[?projects=...][&replicas=...]`)
    /// is what Taurus checks the Apus handshake's `HELLO.action` against.
    pub fn issue_action_certificate(
        &self,
        action: &str,
        projects: Option<&[i64]>,
        replicas: u32,
        lifetime: Duration,
    ) -> Result<IssuedCertificate, CaError> {
        if !identity::is_valid_action_identifier(action) {
            return Err(CaError::InvalidActionIdentifier(action.to_string()));
        }
        let uri = identity::identity_uri(action, projects, replicas);

        let key = KeyPair::generate().map_err(rcgen_err)?;
        let mut params = CertificateParams::new(Vec::<String>::new()).map_err(rcgen_err)?;
        params.is_ca = IsCa::ExplicitNoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        params
            .subject_alt_names
            .push(SanType::URI(Ia5String::try_from(uri).map_err(rcgen_err)?));

        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, action);
        params.distinguished_name = dn;

        let not_after = OffsetDateTime::now_utc() + lifetime;
        params.not_after = not_after;

        let cert = params
            .signed_by(&key, &self.issuer_cert, &self.issuer_key)
            .map_err(rcgen_err)?;

        Ok(IssuedCertificate {
            certificate_pem: cert.pem(),
            private_key_pem: key.serialize_pem(),
            ca_pem: self.ca_pem.clone(),
            not_after_unix: not_after.unix_timestamp(),
        })
    }

    /// Issues a server certificate for a Taurus instance: SANs are the
    /// given endpoint names (hostnames or IP addresses, auto-detected).
    pub fn issue_server_certificate(
        &self,
        endpoint_names: &[String],
        lifetime: Duration,
    ) -> Result<IssuedCertificate, CaError> {
        if endpoint_names.is_empty() {
            return Err(CaError::NoEndpointNames);
        }

        let key = KeyPair::generate().map_err(rcgen_err)?;
        let mut params = CertificateParams::new(endpoint_names.to_vec()).map_err(rcgen_err)?;
        params.is_ca = IsCa::ExplicitNoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, endpoint_names[0].as_str());
        params.distinguished_name = dn;

        let not_after = OffsetDateTime::now_utc() + lifetime;
        params.not_after = not_after;

        let cert = params
            .signed_by(&key, &self.issuer_cert, &self.issuer_key)
            .map_err(rcgen_err)?;

        Ok(IssuedCertificate {
            certificate_pem: cert.pem(),
            private_key_pem: key.serialize_pem(),
            ca_pem: self.ca_pem.clone(),
            not_after_unix: not_after.unix_timestamp(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use rustls::pki_types::CertificateDer;
    use rustls::pki_types::pem::PemObject;
    use x509_parser::extensions::GeneralName;
    use x509_parser::prelude::{FromDer, X509Certificate};

    /// Parses an issued certificate's PEM back for assertions. Leaks the
    /// decoded DER so the parsed certificate can borrow from it for the
    /// static lifetime `X509Certificate` needs - fine, this is test-only.
    fn parse(pem: &str) -> X509Certificate<'static> {
        let der = CertificateDer::from_pem_slice(pem.as_bytes()).expect("valid PEM");
        let der: &'static [u8] = Box::leak(der.as_ref().to_vec().into_boxed_slice());
        X509Certificate::from_der(der).expect("valid DER").1
    }

    #[test]
    fn ephemeral_ca_succeeds_in_development_with_no_config() {
        let ca = CertificateAuthority::load(Environment::Development, None, None);
        assert!(ca.is_ok());
    }

    #[test]
    fn missing_ca_configuration_is_a_hard_error_in_staging_and_production() {
        for environment in [Environment::Staging, Environment::Production] {
            let ca = CertificateAuthority::load(environment, None, None);
            assert!(ca.is_err(), "{environment} should require a configured CA");
        }
    }

    #[test]
    fn only_one_of_cert_or_key_path_is_a_configuration_error() {
        assert!(
            CertificateAuthority::load(Environment::Development, Some("ca.pem"), None).is_err()
        );
        assert!(
            CertificateAuthority::load(Environment::Development, None, Some("ca.key")).is_err()
        );
    }

    #[test]
    fn malformed_action_identifiers_are_rejected() {
        let ca = CertificateAuthority::load(Environment::Development, None, None).unwrap();
        let err = ca
            .issue_action_certificate("not a valid action!", None, 1, Duration::from_secs(60))
            .unwrap_err();
        assert!(matches!(err, CaError::InvalidActionIdentifier(_)));
    }

    #[test]
    fn issued_action_certificate_round_trips_through_x509_parser() {
        let ca = CertificateAuthority::load(Environment::Development, None, None).unwrap();
        let lifetime = Duration::from_secs(3600);
        let issued = ca
            .issue_action_certificate("http-action", Some(&[1, 2]), 3, lifetime)
            .unwrap();

        let cert = parse(&issued.certificate_pem);

        let expected_uri = identity::identity_uri("http-action", Some(&[1, 2]), 3);
        let san = cert.subject_alternative_name().unwrap().unwrap().value;
        let uris: Vec<&str> = san
            .general_names
            .iter()
            .filter_map(|name| match name {
                GeneralName::URI(uri) => Some(*uri),
                _ => None,
            })
            .collect();
        assert_eq!(uris, vec![expected_uri.as_str()]);

        let basic_constraints = cert.basic_constraints().unwrap().unwrap().value;
        assert!(!basic_constraints.ca);

        let eku = cert.extended_key_usage().unwrap().unwrap().value;
        assert!(eku.client_auth);
        assert!(!eku.server_auth);

        let expected_not_after = (OffsetDateTime::now_utc() + lifetime).unix_timestamp();
        assert!((issued.not_after_unix - expected_not_after).abs() <= 2);
        assert_eq!(cert.validity().not_after.timestamp(), issued.not_after_unix);
    }

    #[test]
    fn issued_server_certificate_chains_to_the_ca() {
        use rustls::client::danger::ServerCertVerifier;
        use rustls::pki_types::{ServerName, UnixTime};

        let ca = CertificateAuthority::load(Environment::Development, None, None).unwrap();
        let issued = ca
            .issue_server_certificate(&["localhost".to_string()], Duration::from_secs(3600))
            .unwrap();

        let mut roots = rustls::RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(issued.ca_pem.as_bytes()) {
            roots.add(cert.unwrap()).unwrap();
        }
        let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
            std::sync::Arc::new(roots),
            std::sync::Arc::new(rustls::crypto::ring::default_provider()),
        )
        .build()
        .expect("a root store with the CA should build a verifier");

        let leaf = CertificateDer::from_pem_slice(issued.certificate_pem.as_bytes()).unwrap();
        let server_name = ServerName::try_from("localhost").unwrap();

        verifier
            .verify_server_cert(&leaf, &[], &server_name, &[], UnixTime::now())
            .expect("the issued server certificate should chain to the CA");
    }

    /// A certificate signed by a different, unrelated CA must not verify
    /// against this CA's root store - otherwise the chain check above would
    /// be vacuous (anything verifying, regardless of the actual CA).
    #[test]
    fn a_certificate_from_another_ca_does_not_chain() {
        use rustls::client::danger::ServerCertVerifier;
        use rustls::pki_types::{ServerName, UnixTime};

        let ca = CertificateAuthority::load(Environment::Development, None, None).unwrap();
        let other_ca = CertificateAuthority::ephemeral().unwrap();
        let issued = other_ca
            .issue_server_certificate(&["localhost".to_string()], Duration::from_secs(3600))
            .unwrap();

        let mut roots = rustls::RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(ca.ca_pem().as_bytes()) {
            roots.add(cert.unwrap()).unwrap();
        }
        let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
            std::sync::Arc::new(roots),
            std::sync::Arc::new(rustls::crypto::ring::default_provider()),
        )
        .build()
        .unwrap();

        let leaf = CertificateDer::from_pem_slice(issued.certificate_pem.as_bytes()).unwrap();
        let server_name = ServerName::try_from("localhost").unwrap();

        assert!(
            verifier
                .verify_server_cert(&leaf, &[], &server_name, &[], UnixTime::now())
                .is_err()
        );
    }

    #[test]
    fn issued_server_certificate_has_the_given_sans_and_server_auth() {
        let ca = CertificateAuthority::load(Environment::Development, None, None).unwrap();
        let names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        let issued = ca
            .issue_server_certificate(&names, Duration::from_secs(3600))
            .unwrap();

        let cert = parse(&issued.certificate_pem);
        let san = cert.subject_alternative_name().unwrap().unwrap().value;

        let has_dns_localhost = san
            .general_names
            .iter()
            .any(|name| matches!(name, GeneralName::DNSName(n) if *n == "localhost"));
        let has_ip_loopback = san.general_names.iter().any(|name| match name {
            GeneralName::IPAddress(bytes) => *bytes == [127, 0, 0, 1],
            _ => false,
        });
        assert!(has_dns_localhost);
        assert!(has_ip_loopback);

        let eku = cert.extended_key_usage().unwrap().unwrap().value;
        assert!(eku.server_auth);
        assert!(!eku.client_auth);
    }

    #[test]
    fn server_certificate_requires_at_least_one_endpoint_name() {
        let ca = CertificateAuthority::load(Environment::Development, None, None).unwrap();
        let err = ca
            .issue_server_certificate(&[], Duration::from_secs(60))
            .unwrap_err();
        assert!(matches!(err, CaError::NoEndpointNames));
    }

    #[test]
    fn loading_from_disk_reuses_the_exact_ca_pem_file_contents() {
        let dir = std::env::temp_dir().join(format!("aquila-ca-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let cert_path = dir.join("ca.pem");
        let key_path = dir.join("ca.key");

        let generated = CertificateAuthority::ephemeral().unwrap();
        std::fs::write(&cert_path, generated.ca_pem()).unwrap();
        std::fs::write(&key_path, generated.issuer_key.serialize_pem()).unwrap();

        let loaded = CertificateAuthority::load(
            Environment::Production,
            cert_path.to_str(),
            key_path.to_str(),
        )
        .unwrap();

        assert_eq!(loaded.ca_pem(), generated.ca_pem());

        let issued = loaded
            .issue_action_certificate("http-action", None, 1, Duration::from_secs(60))
            .unwrap();
        assert_eq!(issued.ca_pem, generated.ca_pem());

        std::fs::remove_dir_all(&dir).ok();
    }
}
