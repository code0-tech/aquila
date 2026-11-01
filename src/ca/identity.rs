//! The action identity URI carried in the URI SAN of every client
//! certificate Aquila issues, and the validation rule for the action
//! identifier it embeds.
//!
//! **Must stay byte-for-byte in sync with `apus-protocol::tls`** (the
//! `apus` repo, `rust/apus-protocol/src/tls.rs`): that crate is what Taurus
//! uses to parse the identity back out of the certificate it receives
//! during the Apus handshake (`peer_identity`/`parse_identity`). Aquila
//! does not depend on `apus-protocol` at runtime (see the decision recorded
//! in the `feat/init-apus` branch), so this is a deliberate, reviewed copy
//! rather than a shared dependency - any change to the URI format on either
//! side must be mirrored on the other.

/// Scheme and path prefix of the identity URI in an action's client
/// certificate: `code0://action/<identifier>[?projects=1,2][&replicas=3]`.
pub const IDENTITY_PREFIX: &str = "code0://action/";

/// Builds the identity URI for an action's client certificate. Mirrors
/// `apus_protocol::tls::identity_uri` exactly.
pub fn identity_uri(action: &str, projects: Option<&[i64]>, replicas: u32) -> String {
    let mut uri = format!("{IDENTITY_PREFIX}{action}");
    let mut query = Vec::new();
    if let Some(projects) = projects {
        query.push(format!(
            "projects={}",
            projects
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if replicas > 1 {
        query.push(format!("replicas={replicas}"));
    }
    if !query.is_empty() {
        uri.push('?');
        uri.push_str(&query.join("&"));
    }
    uri
}

/// Whether `action` is a valid identifier to embed in an identity URI -
/// mirrors the character class `apus_protocol::tls::parse_identity` accepts
/// (`[A-Za-z0-9._-]`, 1..=255 bytes). Aquila must never issue a certificate
/// Taurus's parser would then reject.
pub fn is_valid_action_identifier(action: &str) -> bool {
    let valid = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
    !action.is_empty() && action.len() <= 255 && action.chars().all(valid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_uri_with_no_projects_or_replicas() {
        assert_eq!(
            identity_uri("http-action", None, 1),
            "code0://action/http-action"
        );
    }

    #[test]
    fn identity_uri_with_projects_only() {
        assert_eq!(
            identity_uri("http-action", Some(&[1, 2]), 1),
            "code0://action/http-action?projects=1,2"
        );
    }

    #[test]
    fn identity_uri_with_replicas_only() {
        assert_eq!(
            identity_uri("http-action", None, 3),
            "code0://action/http-action?replicas=3"
        );
    }

    #[test]
    fn identity_uri_with_projects_and_replicas() {
        assert_eq!(
            identity_uri("http-action", Some(&[1, 2]), 3),
            "code0://action/http-action?projects=1,2&replicas=3"
        );
    }

    #[test]
    fn identity_uri_replicas_of_one_is_omitted_even_with_projects() {
        assert_eq!(
            identity_uri("a", Some(&[7]), 1),
            "code0://action/a?projects=7"
        );
    }

    #[test]
    fn identity_uri_single_project() {
        assert_eq!(
            identity_uri("a", Some(&[42]), 1),
            "code0://action/a?projects=42"
        );
    }

    #[test]
    fn valid_action_identifiers_are_accepted() {
        for action in ["http-action", "a", "a.b.c", "a_b-c.1", &"a".repeat(255)] {
            assert!(is_valid_action_identifier(action), "{action}");
        }
    }

    #[test]
    fn malformed_action_identifiers_are_rejected() {
        for action in ["", "a/b", "a?b", "a b", "a:b", &"a".repeat(256)] {
            assert!(!is_valid_action_identifier(action), "{action}");
        }
    }
}
