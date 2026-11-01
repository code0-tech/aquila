//! Hand-written tonic server plumbing for `aquila.apus.CertificateService`,
//! mirroring the shape `tonic-build`/`tonic-prost-build` generates for
//! tucana's services (e.g. `tucana::aquila::module_service_server`) byte
//! for byte, with only the names and the (single) RPC changed. Kept
//! separate from [`super::service_impl`] so the generated-style
//! boilerplate doesn't obscure the actual business logic.
#![allow(clippy::wildcard_imports)]

use super::messages::{IssueCertificateRequest, IssueCertificateResponse};
use tonic::codegen::*;

/// Implemented by the business-logic server; see [`super::service_impl`].
#[tonic::async_trait]
pub trait CertificateService: std::marker::Send + std::marker::Sync + 'static {
    async fn issue_certificate(
        &self,
        request: tonic::Request<IssueCertificateRequest>,
    ) -> std::result::Result<tonic::Response<IssueCertificateResponse>, tonic::Status>;
}

#[derive(Debug)]
pub struct CertificateServiceServer<T> {
    inner: Arc<T>,
    accept_compression_encodings: EnabledCompressionEncodings,
    send_compression_encodings: EnabledCompressionEncodings,
    max_decoding_message_size: Option<usize>,
    max_encoding_message_size: Option<usize>,
}

impl<T> CertificateServiceServer<T> {
    pub fn new(inner: T) -> Self {
        Self::from_arc(Arc::new(inner))
    }

    pub fn from_arc(inner: Arc<T>) -> Self {
        Self {
            inner,
            accept_compression_encodings: Default::default(),
            send_compression_encodings: Default::default(),
            max_decoding_message_size: None,
            max_encoding_message_size: None,
        }
    }

    pub fn with_interceptor<F>(inner: T, interceptor: F) -> InterceptedService<Self, F>
    where
        F: tonic::service::Interceptor,
    {
        InterceptedService::new(Self::new(inner), interceptor)
    }
}

impl<T, B> tonic::codegen::Service<http::Request<B>> for CertificateServiceServer<T>
where
    T: CertificateService,
    B: Body + std::marker::Send + 'static,
    B::Error: Into<StdError> + std::marker::Send + 'static,
{
    type Response = http::Response<tonic::body::Body>;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        match req.uri().path() {
            "/aquila.apus.CertificateService/IssueCertificate" => {
                #[allow(non_camel_case_types)]
                struct IssueCertificateSvc<T: CertificateService>(pub Arc<T>);
                impl<T: CertificateService> tonic::server::UnaryService<IssueCertificateRequest>
                    for IssueCertificateSvc<T>
                {
                    type Response = IssueCertificateResponse;
                    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

                    fn call(
                        &mut self,
                        request: tonic::Request<IssueCertificateRequest>,
                    ) -> Self::Future {
                        let inner = Arc::clone(&self.0);
                        let fut = async move {
                            <T as CertificateService>::issue_certificate(&inner, request).await
                        };
                        Box::pin(fut)
                    }
                }

                let accept_compression_encodings = self.accept_compression_encodings;
                let send_compression_encodings = self.send_compression_encodings;
                let max_decoding_message_size = self.max_decoding_message_size;
                let max_encoding_message_size = self.max_encoding_message_size;
                let inner = self.inner.clone();
                let fut = async move {
                    let method = IssueCertificateSvc(inner);
                    let codec = tonic_prost::ProstCodec::default();
                    let mut grpc = tonic::server::Grpc::new(codec)
                        .apply_compression_config(
                            accept_compression_encodings,
                            send_compression_encodings,
                        )
                        .apply_max_message_size_config(
                            max_decoding_message_size,
                            max_encoding_message_size,
                        );
                    let res = grpc.unary(method, req).await;
                    Ok(res)
                };
                Box::pin(fut)
            }
            _ => Box::pin(async move {
                let mut response = http::Response::new(tonic::body::Body::default());
                let headers = response.headers_mut();
                headers.insert(
                    tonic::Status::GRPC_STATUS,
                    (tonic::Code::Unimplemented as i32).into(),
                );
                headers.insert(
                    http::header::CONTENT_TYPE,
                    tonic::metadata::GRPC_CONTENT_TYPE,
                );
                Ok(response)
            }),
        }
    }
}

impl<T> Clone for CertificateServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            accept_compression_encodings: self.accept_compression_encodings,
            send_compression_encodings: self.send_compression_encodings,
            max_decoding_message_size: self.max_decoding_message_size,
            max_encoding_message_size: self.max_encoding_message_size,
        }
    }
}

pub const SERVICE_NAME: &str = "aquila.apus.CertificateService";

impl<T> tonic::server::NamedService for CertificateServiceServer<T> {
    const NAME: &'static str = SERVICE_NAME;
}
