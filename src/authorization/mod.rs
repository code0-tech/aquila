//! Bearer-token helpers shared by every gRPC client and server in Aquila:
//! [`authorization::get_authorization_metadata`] to attach a token to an
//! outgoing request, [`authorization::extract_token`] to read one back off
//! an incoming request.

pub mod authorization {
    use std::str::FromStr;
    use tonic::{
        Request, Status,
        metadata::{MetadataMap, MetadataValue},
    };

    /// get_authorization_metadata
    ///
    /// Creates a `MetadataMap` that contains the defined token as a value of the `authorization` key
    /// Used for setting the runtime_token to authorize Sagittarius request
    ///
    /// # Examples
    ///
    /// ```
    /// use aquila_grpc::get_authorization_metadata;
    /// let token = String::from("token");
    /// let metadata = get_authorization_metadata(&token);
    /// assert!(metadata.get("authorization").is_some());
    /// assert_eq!(metadata.get("authorization").unwrap(), "token");
    /// ```
    pub fn get_authorization_metadata(token: &str) -> MetadataMap {
        let metadata_value = MetadataValue::from_str(token).unwrap_or_else(|error| {
            panic!(
                "An error occurred trying to convert runtime_token into metadata: {}",
                error
            );
        });

        let mut map = MetadataMap::new();
        map.insert("authorization", metadata_value);
        map
    }

    /// Reads and validates the bearer token off an incoming request's
    /// `authorization` header. Generic over the request body type so it
    /// works for both unary and streaming gRPC requests.
    pub fn extract_token<T>(request: &Request<T>) -> Result<&str, Status> {
        let header = request.metadata().get("authorization").ok_or_else(|| {
            log::warn!("Missing authorization header");
            Status::unauthenticated("missing authorization header")
        })?;

        let token = header.to_str().map_err(|_| {
            log::warn!("Authorization header is not valid ASCII");
            Status::unauthenticated("authorization header is not valid ASCII")
        })?;

        if token.is_empty() {
            log::warn!("Authorization token is empty");
            return Err(Status::unauthenticated("authorization token is empty"));
        }

        Ok(token)
    }
}
