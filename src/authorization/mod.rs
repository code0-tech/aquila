//! Bearer-token helpers shared by every gRPC client and server in Aquila:
//! [`authorization::get_authentication_metadata`] to attach a token to an
//! outgoing request, [`authorization::extract_token`] to read one back off
//! an incoming request, and [`authorization::verify_jwt`] to check a
//! presented token against the pre-provisioned secret for an action or
//! runtime identity.

pub mod authorization {
    use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
    use serde::Deserialize;
    use std::str::FromStr;
    use tonic::{
        Request, Status,
        metadata::{MetadataMap, MetadataValue},
    };

    /// Claims carried by an action/runtime authentication JWT. `sub` must
    /// match the identity (action identifier or runtime family) the token is
    /// presented for, so a secret leaked for one identity can't be replayed
    /// to authenticate as another.
    #[derive(Debug, Deserialize)]
    struct Claims {
        sub: String,
        #[allow(dead_code)]
        exp: u64,
    }

    /// get_authentication_metadata
    ///
    /// Creates a `MetadataMap` that contains the defined token as a value of the `authentication` key.
    /// Used for setting the runtime_token to authenticate Aquila against the Sagittarius gateway.
    ///
    /// # Examples
    ///
    /// ```
    /// use aquila_grpc::get_authentication_metadata;
    /// let token = String::from("token");
    /// let metadata = get_authentication_metadata(&token);
    /// assert!(metadata.get("authentication").is_some());
    /// assert_eq!(metadata.get("authentication").unwrap(), "token");
    /// ```
    pub fn get_authentication_metadata(token: &str) -> MetadataMap {
        let metadata_value = MetadataValue::from_str(token).unwrap_or_else(|error| {
            panic!(
                "An error occurred trying to convert runtime_token into metadata: {}",
                error
            );
        });

        let mut map = MetadataMap::new();
        map.insert("authentication", metadata_value);
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

    /// Verifies `token` is a JWT signed with `secret` (HS256), not expired,
    /// and issued for `expected_subject` - the pre-provisioned identity
    /// (action identifier or runtime family) `secret` was configured for.
    ///
    /// # Examples
    ///
    /// ```
    /// use aquila_grpc::verify_jwt;
    /// use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct Claims { sub: String, exp: u64 }
    ///
    /// let token = encode(
    ///     &Header::new(Algorithm::HS256),
    ///     &Claims { sub: "taurus".to_string(), exp: 4_102_444_800 },
    ///     &EncodingKey::from_secret(b"secret"),
    /// ).unwrap();
    ///
    /// assert!(verify_jwt(&token, "secret", "taurus"));
    /// assert!(!verify_jwt(&token, "secret", "draco-rest"));
    /// assert!(!verify_jwt(&token, "wrong-secret", "taurus"));
    /// ```
    pub fn verify_jwt(token: &str, secret: &str, expected_subject: &str) -> bool {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_required_spec_claims(&["sub", "exp"]);
        let decoding_key = DecodingKey::from_secret(secret.as_bytes());

        match decode::<Claims>(token, &decoding_key, &validation) {
            Ok(data) => data.claims.sub == expected_subject,
            Err(err) => {
                log::debug!("JWT verification failed: {:?}", err);
                false
            }
        }
    }
}
