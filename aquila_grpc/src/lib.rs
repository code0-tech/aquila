use std::str::FromStr;
use tonic::metadata::{MetadataMap, MetadataValue};

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
pub fn get_authorization_metadata(token: &String) -> MetadataMap {
    let metadata_value = match MetadataValue::from_str(token) {
        Ok(value) => value,
        Err(error) => {
            panic!(
                "An error occurred trying to convert runtime_token into metadata: {}",
                error
            );
        }
    };

    let mut map = MetadataMap::new();
    map.insert("authorization", metadata_value);
    map
}
