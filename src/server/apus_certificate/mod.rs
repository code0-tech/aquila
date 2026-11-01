//! Aquila's own certificate-issuance RPC for Apus: a hand-written, local
//! gRPC service (see [`server`]'s module doc for why it's hand-written
//! rather than generated), fronting [`crate::ca::CertificateAuthority`].
//!
//! Not part of tucana: there is no certificate message in the tucana
//! 0.0.83 proto, and this crate has no `build.rs`/`tonic-build` step of its
//! own to extend it from here (see the module doc on [`crate::server`]).

pub mod messages;
pub mod server;
pub mod service_impl;

pub use messages::{IssueCertificateRequest, IssueCertificateResponse, SubjectKind};
pub use server::{CertificateService, CertificateServiceServer};
pub use service_impl::AquilaCertificateServiceServer;
