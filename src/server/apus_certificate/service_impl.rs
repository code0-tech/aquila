//! Business logic for `aquila.apus.CertificateService.IssueCertificate`:
//! authenticates the caller's JWT (the same bearer-token scheme every other
//! Aquila RPC uses), authorizes it for the requested certificate kind, and
//! calls into [`crate::ca::CertificateAuthority`] to issue it.
//!
//! Authorization is two-phase, so the error maps to the right gRPC status:
//! 1. Is the token valid for *some* identity matching `identifier`, action
//!    or runtime? If not - wrong secret, or the identifier isn't
//!    provisioned at all - [`tonic::Status::unauthenticated`].
//! 2. Is that identity authorized for the *requested* [`SubjectKind`]? An
//!    action's own, valid token asking for a `Server` certificate (or a
//!    runtime's asking for an `Action` one) is a real identity that simply
//!    isn't allowed to do this - [`tonic::Status::permission_denied`].
use std::time::Duration;

use tonic::{Request, Response, Status};

use crate::{
    authorization::authorization::extract_token, ca::CertificateAuthority,
    configuration::service::ServiceConfiguration,
};

use super::{
    messages::{IssueCertificateRequest, IssueCertificateResponse, SubjectKind},
    server::CertificateService,
};

pub struct AquilaCertificateServiceServer {
    ca: std::sync::Arc<CertificateAuthority>,
    service_configuration: ServiceConfiguration,
    action_cert_lifetime: Duration,
    server_cert_lifetime: Duration,
}

impl AquilaCertificateServiceServer {
    pub fn new(
        ca: std::sync::Arc<CertificateAuthority>,
        service_configuration: ServiceConfiguration,
        action_cert_lifetime: Duration,
        server_cert_lifetime: Duration,
    ) -> Self {
        Self {
            ca,
            service_configuration,
            action_cert_lifetime,
            server_cert_lifetime,
        }
    }
}

#[tonic::async_trait]
impl CertificateService for AquilaCertificateServiceServer {
    #[tracing::instrument(
        name = "aquila.apus.certificate.issue",
        skip_all,
        fields(rpc.system = "grpc", rpc.service = "CertificateService", rpc.method = "IssueCertificate")
    )]
    async fn issue_certificate(
        &self,
        request: Request<IssueCertificateRequest>,
    ) -> Result<Response<IssueCertificateResponse>, Status> {
        let token = match extract_token(&request) {
            Ok(token) => token.to_string(),
            Err(status) => {
                log::warn!("Rejected certificate request reason=missing_or_invalid_token");
                return Err(status);
            }
        };

        let request = request.into_inner();
        let subject_kind = SubjectKind::try_from(request.subject_kind)
            .map_err(|_| Status::invalid_argument("unknown subject_kind"))?;

        if !self
            .service_configuration
            .has_service(&token, &request.identifier)
        {
            log::warn!(
                "Rejected certificate request reason=token_not_registered identifier={}",
                request.identifier
            );
            return Err(Status::unauthenticated(
                "token is not valid for this identifier",
            ));
        }

        match subject_kind {
            SubjectKind::Action => {
                if !self
                    .service_configuration
                    .has_action(&token, &request.identifier)
                {
                    log::warn!(
                        "Rejected certificate request reason=not_an_action identifier={}",
                        request.identifier
                    );
                    return Err(Status::permission_denied(
                        "token is not authorized to request an action certificate",
                    ));
                }

                // The identity URI's `projects`/`replicas` are Aquila's
                // trust boundary for Apus routing (see `crate::ca::identity`),
                // so the project scope is taken from the caller as given -
                // there's no existing central record of "which projects does
                // this action serve" to validate it against (unlike
                // `replicas`, which Aquila already owns via the service
                // configuration and uses here instead of trusting the
                // request). A malicious action can presently ask for a
                // certificate scoped to any `projects` it likes; narrowing
                // that is flagged as a follow-up in this change's summary.
                let projects = if request.projects.is_empty() {
                    None
                } else {
                    Some(request.projects.as_slice())
                };
                let replicas = self
                    .service_configuration
                    .action_replicas(&token, &request.identifier);

                let issued = self
                    .ca
                    .issue_action_certificate(
                        &request.identifier,
                        projects,
                        replicas,
                        self.action_cert_lifetime,
                    )
                    .map_err(|error| {
                        log::error!(
                            "Failed to issue action certificate identifier={} error={error}",
                            request.identifier
                        );
                        Status::internal(error.to_string())
                    })?;

                log::info!(
                    "Issued action certificate identifier={}",
                    request.identifier
                );
                Ok(Response::new(issued.into()))
            }
            SubjectKind::Server => {
                if !self
                    .service_configuration
                    .has_runtime(&token, &request.identifier)
                {
                    log::warn!(
                        "Rejected certificate request reason=not_a_runtime identifier={}",
                        request.identifier
                    );
                    return Err(Status::permission_denied(
                        "token is not authorized to request a server certificate",
                    ));
                }
                if request.endpoint_names.is_empty() {
                    return Err(Status::invalid_argument(
                        "endpoint_names must not be empty for a server certificate",
                    ));
                }

                let issued = self
                    .ca
                    .issue_server_certificate(&request.endpoint_names, self.server_cert_lifetime)
                    .map_err(|error| {
                        log::error!(
                            "Failed to issue server certificate identifier={} error={error}",
                            request.identifier
                        );
                        Status::internal(error.to_string())
                    })?;

                log::info!(
                    "Issued server certificate identifier={}",
                    request.identifier
                );
                Ok(Response::new(issued.into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use serde::Serialize;
    use std::sync::Arc;

    #[derive(Serialize)]
    struct Claims {
        sub: String,
        exp: u64,
    }

    fn jwt(secret: &str, subject: &str) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            &Claims {
                sub: subject.to_string(),
                exp: 4_102_444_800,
            },
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    /// Builds a [`ServiceConfiguration`] through its real, public loader
    /// (`from_path`) rather than constructing the domain type directly -
    /// `configuration::service`'s wire-format DTOs are deliberately private
    /// to that module (see `configuration/service/dto.rs`), so this is the
    /// only way to build a fixture from outside it.
    fn fixture() -> ServiceConfiguration {
        let dir =
            std::env::temp_dir().join(format!("aquila-apus-cert-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("service.json");
        std::fs::write(
            &path,
            r#"{
                "actions": [{"token": "action-secret", "identifier": "action-identifier", "configs": [], "replicas": 3}],
                "runtimes": [{"token": "taurus-secret", "identifier": "taurus", "resolved_modules": []}]
            }"#,
        )
        .unwrap();

        let config = ServiceConfiguration::from_path(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        config
    }

    fn server() -> AquilaCertificateServiceServer {
        let ca = CertificateAuthority::load(
            crate::configuration::env::Environment::Development,
            None,
            None,
        )
        .unwrap();
        AquilaCertificateServiceServer::new(
            Arc::new(ca),
            fixture(),
            Duration::from_secs(60),
            Duration::from_secs(60),
        )
    }

    fn request(
        token: &str,
        subject_kind: SubjectKind,
        identifier: &str,
        endpoint_names: Vec<String>,
    ) -> Request<IssueCertificateRequest> {
        let mut request = Request::new(IssueCertificateRequest {
            subject_kind: subject_kind as i32,
            identifier: identifier.to_string(),
            projects: vec![],
            endpoint_names,
        });
        request
            .metadata_mut()
            .insert("authorization", token.parse().unwrap());
        request
    }

    #[tokio::test]
    async fn valid_action_jwt_issues_a_matching_client_certificate() {
        let server = server();
        let token = jwt("action-secret", "action-identifier");
        let response = server
            .issue_certificate(request(
                &token,
                SubjectKind::Action,
                "action-identifier",
                vec![],
            ))
            .await
            .unwrap()
            .into_inner();

        assert!(response.certificate_pem.contains("BEGIN CERTIFICATE"));
        assert!(!response.ca_pem.is_empty());
    }

    #[tokio::test]
    async fn wrong_secret_is_unauthenticated() {
        let server = server();
        let token = jwt("wrong-secret", "action-identifier");
        let status = server
            .issue_certificate(request(
                &token,
                SubjectKind::Action,
                "action-identifier",
                vec![],
            ))
            .await
            .unwrap_err();

        assert_eq!(status.code(), tonic::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn valid_taurus_jwt_issues_a_server_auth_certificate() {
        let server = server();
        let token = jwt("taurus-secret", "taurus");
        let response = server
            .issue_certificate(request(
                &token,
                SubjectKind::Server,
                "taurus",
                vec!["localhost".to_string()],
            ))
            .await
            .unwrap()
            .into_inner();

        assert!(response.certificate_pem.contains("BEGIN CERTIFICATE"));
    }

    #[tokio::test]
    async fn action_jwt_requesting_a_server_certificate_is_permission_denied() {
        let server = server();
        let token = jwt("action-secret", "action-identifier");
        let status = server
            .issue_certificate(request(
                &token,
                SubjectKind::Server,
                "action-identifier",
                vec!["localhost".to_string()],
            ))
            .await
            .unwrap_err();

        assert_eq!(status.code(), tonic::Code::PermissionDenied);
    }

    #[tokio::test]
    async fn missing_authorization_header_is_unauthenticated() {
        let server = server();
        let request = Request::new(IssueCertificateRequest {
            subject_kind: SubjectKind::Action as i32,
            identifier: "action-identifier".to_string(),
            projects: vec![],
            endpoint_names: vec![],
        });

        let status = server.issue_certificate(request).await.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn server_certificate_without_endpoint_names_is_invalid_argument() {
        let server = server();
        let token = jwt("taurus-secret", "taurus");
        let status = server
            .issue_certificate(request(&token, SubjectKind::Server, "taurus", vec![]))
            .await
            .unwrap_err();

        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }
}
