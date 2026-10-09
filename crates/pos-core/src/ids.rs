//! Sync-safe identifiers: UUIDv7 strings, time-ordered, unique across tills (spec §5).

use uuid::Uuid;

/// A new UUIDv7 as a 36-char lower-case string.
pub fn new_id() -> String {
    Uuid::now_v7().to_string()
}

/// True when `s` is an id as [`new_id`] writes it: a version 7 UUID in canonical form
/// (36 chars, hyphenated, lower-case hex). Upper case, other versions and nil are rejected.
pub fn is_valid_id(s: &str) -> bool {
    s.len() == 36
        && !s.bytes().any(|b| b.is_ascii_uppercase())
        && Uuid::parse_str(s).is_ok_and(|id| id.get_version_num() == 7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_uuid_v7_strings() {
        let id = new_id();
        assert_eq!(id.len(), 36);
        assert!(is_valid_id(&id));
        assert_eq!(uuid::Uuid::parse_str(&id).unwrap().get_version_num(), 7);
        assert!(!is_valid_id("not-an-id"));
    }

    #[test]
    fn ids_sort_in_creation_order() {
        let ids: Vec<String> = (0..1_000).map(|_| new_id()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn ids_are_lower_case_and_distinct() {
        let (a, b) = (new_id(), new_id());
        assert_ne!(a, b);
        assert_eq!(a, a.to_lowercase());
    }

    #[test]
    fn only_the_hyphenated_form_is_valid() {
        let id = new_id();
        assert!(!is_valid_id(&id.replace('-', "")));
        assert!(!is_valid_id(&format!("{{{id}}}")));
        assert!(!is_valid_id(&format!("urn:uuid:{id}")));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz"));
    }

    #[test]
    fn only_lower_case_version_7_is_valid() {
        let id = new_id();
        assert!(is_valid_id(&id.to_lowercase()));
        assert!(is_valid_id("01890a5d-ac96-774b-bcce-b302099a8057"));
        assert!(!is_valid_id("01890A5D-AC96-774B-BCCE-B302099A8057"));
        assert!(!is_valid_id("00000000-0000-0000-0000-000000000000"));
        // A version 4 UUID.
        assert!(!is_valid_id("550e8400-e29b-41d4-a716-446655440000"));
    }
}
