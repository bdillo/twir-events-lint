use std::{
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_twir-events-lint"))
        .args(args)
        .output()
        .expect("failed to run twir-events-lint")
}

fn output_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn temporary_copy(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "twir-events-lint-{}-{unique}.md",
        std::process::id()
    ));
    std::fs::copy(fixture(name), &path).expect("failed to create temporary draft");
    path
}

#[test]
fn valid_draft_succeeds() {
    let draft = fixture("valid.md");
    let output = run(&["--draft", draft.to_str().unwrap()]);

    assert!(
        output.status.success(),
        "stderr: {}",
        output_text(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output_text(&output.stderr).contains("lgtm!"));
}

#[test]
fn invalid_draft_reports_the_lint_error() {
    let draft = fixture("invalid.md");
    let output = run(&["--draft", draft.to_str().unwrap()]);
    let stderr = output_text(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("event date '2024-10-01' does not fall within newsletter date range"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("line #8"), "stderr: {stderr}");
}

#[test]
fn fix_removes_out_of_range_event_and_empty_region() {
    let draft = temporary_copy("fixable.md");

    let output = run(&["--draft", draft.to_str().unwrap(), "--fix"]);
    let fixed = std::fs::read_to_string(&draft).expect("failed to read fixed draft");
    std::fs::remove_file(&draft).expect("failed to remove temporary draft");

    assert!(
        output.status.success(),
        "stderr: {}",
        output_text(&output.stderr)
    );
    assert!(!fixed.contains("Old Event"));
    assert!(!fixed.contains("### Europe"));
    assert!(fixed.contains("Test Event"));
}

#[test]
fn fix_removes_and_reorders_events_idempotently() {
    let draft = temporary_copy("fixable-ordering.md");

    let first_output = run(&["--draft", draft.to_str().unwrap(), "--fix"]);
    let fixed = std::fs::read_to_string(&draft).expect("failed to read fixed draft");
    let second_output = run(&["--draft", draft.to_str().unwrap(), "--fix"]);
    let fixed_again = std::fs::read_to_string(&draft).expect("failed to reread fixed draft");
    std::fs::remove_file(&draft).expect("failed to remove temporary draft");

    assert!(
        first_output.status.success(),
        "stderr: {}",
        output_text(&first_output.stderr)
    );
    assert!(
        second_output.status.success(),
        "stderr: {}",
        output_text(&second_output.stderr)
    );
    assert!(!fixed.contains("Old Event"));
    assert!(fixed.find("Earlier Group").unwrap() < fixed.find("Later Group").unwrap());
    assert!(fixed.contains(
        "    * [**Earlier Event**](https://example.com/events/earlier/) + [**Workshop**](https://example.com/events/workshop/)"
    ));
    assert_eq!(fixed_again, fixed);
}

#[test]
fn merge_output_matches_expected_markdown() {
    let draft = fixture("valid.md");
    let events = fixture("events.json");
    let expected = std::fs::read_to_string(fixture("expected.md"))
        .expect("failed to read expected merge output");
    let output = run(&[
        "--draft",
        draft.to_str().unwrap(),
        "--new-events-file",
        events.to_str().unwrap(),
    ]);

    assert!(
        output.status.success(),
        "stderr: {}",
        output_text(&output.stderr)
    );
    assert_eq!(output_text(&output.stdout), expected);
}

#[test]
fn merge_can_update_the_draft_in_place() {
    let draft = temporary_copy("valid.md");
    let events = fixture("events.json");
    let expected = std::fs::read_to_string(fixture("expected-updated.md"))
        .expect("failed to read expected updated draft");

    let output = run(&[
        "--draft",
        draft.to_str().unwrap(),
        "--new-events-file",
        events.to_str().unwrap(),
        "--in-place",
    ]);
    let updated = std::fs::read_to_string(&draft).expect("failed to read updated draft");
    std::fs::remove_file(&draft).expect("failed to remove temporary draft");

    assert!(
        output.status.success(),
        "stderr: {}",
        output_text(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert_eq!(updated, expected);
}

#[test]
fn merge_can_insert_into_a_draft_without_existing_events() {
    let draft = temporary_copy("no-events.md");
    let events = fixture("events.json");
    let expected = std::fs::read_to_string(fixture("expected-updated.md"))
        .expect("failed to read expected updated draft");

    let output = run(&[
        "--draft",
        draft.to_str().unwrap(),
        "--new-events-file",
        events.to_str().unwrap(),
        "--in-place",
    ]);
    let updated = std::fs::read_to_string(&draft).expect("failed to read updated draft");
    std::fs::remove_file(&draft).expect("failed to remove temporary draft");

    assert!(
        output.status.success(),
        "stderr: {}",
        output_text(&output.stderr)
    );
    assert_eq!(updated, expected);
}

#[test]
fn out_of_range_incoming_events_are_logged_and_dropped() {
    let draft = fixture("valid.md");
    let events = fixture("out-of-range-events.json");
    let output = run(&[
        "--draft",
        draft.to_str().unwrap(),
        "--new-events-file",
        events.to_str().unwrap(),
    ]);
    let stdout = output_text(&output.stdout);
    let stderr = output_text(&output.stderr);

    assert!(output.status.success(), "stderr: {stderr}");
    assert!(!stdout.contains("Old Incoming Event"));
    assert!(stdout.contains("Test Event"));
    assert!(
        stderr.contains("dropping out-of-range incoming event 'Old Incoming Event' in Europe"),
        "stderr: {stderr}"
    );
}

#[test]
fn missing_events_header_reports_an_error() {
    let draft = fixture("missing-header.md");
    let output = run(&["--draft", draft.to_str().unwrap()]);
    let stderr = output_text(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("header '## Upcoming Events' not found"),
        "stderr: {stderr}"
    );
}

#[test]
fn invalid_merged_document_is_not_written() {
    let draft = temporary_copy("valid.md");
    let original = std::fs::read_to_string(&draft).unwrap();
    let events = fixture("unrenderable-events.json");
    let output = run(&[
        "--draft",
        draft.to_str().unwrap(),
        "--new-events-file",
        events.to_str().unwrap(),
        "--in-place",
    ]);
    let unchanged = std::fs::read_to_string(&draft).unwrap();
    std::fs::remove_file(&draft).expect("failed to remove temporary draft");

    assert!(!output.status.success());
    assert_eq!(unchanged, original);
    assert!(
        output_text(&output.stderr).contains("merged document is invalid"),
        "stderr: {}",
        output_text(&output.stderr)
    );
}

#[test]
fn malformed_events_json_reports_the_file() {
    let draft = temporary_copy("valid.md");
    let original = std::fs::read_to_string(&draft).unwrap();
    let events = fixture("malformed-events.json");
    let output = run(&[
        "--draft",
        draft.to_str().unwrap(),
        "--new-events-file",
        events.to_str().unwrap(),
        "--in-place",
    ]);
    let stderr = output_text(&output.stderr);
    let unchanged = std::fs::read_to_string(&draft).unwrap();
    std::fs::remove_file(&draft).expect("failed to remove temporary draft");

    assert!(!output.status.success());
    assert_eq!(unchanged, original);
    assert!(stderr.contains("failed to parse"), "stderr: {stderr}");
    assert!(stderr.contains("malformed-events.json"), "stderr: {stderr}");
}
