//! Wire messages for the hand-written `aquila.apus.CertificateService`.
//!
//! There is no `.proto` for this service and no `tonic-build`/`prost-build`
//! codegen step in this crate (it only consumes tucana's pre-generated
//! code, see `crate::server::mod`'s doc comment) - these types are written
//! by hand to the same shape `prost-build` would produce, so they encode
//! and decode exactly like a generated message would. If this service
//! grows, the shape here is the first thing to move into a real `.proto`.

/// Which kind of certificate is being requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ::prost::Enumeration)]
#[repr(i32)]
pub enum SubjectKind {
    /// A short-lived client certificate identifying one action.
    Action = 0,
    /// A server certificate for a Taurus instance.
    Server = 1,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IssueCertificateRequest {
    #[prost(enumeration = "SubjectKind", tag = "1")]
    pub subject_kind: i32,
    /// The action identifier (`SubjectKind::Action`) or runtime family
    /// (`SubjectKind::Server`, e.g. `taurus`) this certificate is for.
    #[prost(string, tag = "2")]
    pub identifier: String,
    /// `SubjectKind::Action` only: the projects to scope the certificate's
    /// identity to. Empty means every project of the action - *not* "no
    /// projects" - mirroring `identity_uri`'s `None` (no `projects=` query
    /// parameter at all).
    #[prost(int64, repeated, tag = "3")]
    pub projects: ::prost::alloc::vec::Vec<i64>,
    /// `SubjectKind::Server` only: the DNS names/IP addresses to put in the
    /// server certificate's SAN.
    #[prost(string, repeated, tag = "4")]
    pub endpoint_names: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IssueCertificateResponse {
    #[prost(string, tag = "1")]
    pub certificate_pem: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub private_key_pem: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub ca_pem: ::prost::alloc::string::String,
    /// Unix seconds after which the certificate is no longer valid.
    #[prost(int64, tag = "4")]
    pub not_after_unix: i64,
}

impl From<crate::ca::IssuedCertificate> for IssueCertificateResponse {
    fn from(issued: crate::ca::IssuedCertificate) -> Self {
        Self {
            certificate_pem: issued.certificate_pem,
            private_key_pem: issued.private_key_pem,
            ca_pem: issued.ca_pem,
            not_after_unix: issued.not_after_unix,
        }
    }
}
