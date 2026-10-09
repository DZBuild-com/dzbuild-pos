//! The two-PC LAN checklist is the only place where the LAN is proved on real hardware: it must sit
//! at the path the release checklist names, under one name only, and keep its 15 numbered lines.

#[test]
fn the_two_pc_checklist_is_where_the_release_checklist_names_it() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let checklist = "docs/testing/smoke/m5-two-pc-smoke.md";
    let body = std::fs::read_to_string(root.join(checklist))
        .unwrap_or_else(|e| panic!("{checklist} is missing: {e}"));
    let release = std::fs::read_to_string(root.join("docs/release-checklist.md"))
        .unwrap_or_else(|e| panic!("docs/release-checklist.md is missing: {e}"));
    assert!(
        release.contains(checklist),
        "the release checklist points at {checklist}"
    );
    assert!(
        !root
            .join("docs/testing/smoke/m5-two-pc-checklist.md")
            .exists(),
        "one two-PC checklist, under one name"
    );
    for line in 1..=15 {
        assert!(
            body.lines().any(|l| l.starts_with(&format!("{line}. "))),
            "the checklist keeps line {line}"
        );
    }
}
