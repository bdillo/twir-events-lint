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
    let source = fixture("fixable.md");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let draft = std::env::temp_dir().join(format!(
        "twir-events-lint-{}-{unique}.md",
        std::process::id()
    ));
    std::fs::copy(&source, &draft).expect("failed to create temporary draft");

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
fn malformed_events_json_reports_the_file() {
    let draft = fixture("valid.md");
    let events = fixture("malformed-events.json");
    let output = run(&[
        "--draft",
        draft.to_str().unwrap(),
        "--new-events-file",
        events.to_str().unwrap(),
    ]);
    let stderr = output_text(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("failed to parse"), "stderr: {stderr}");
    assert!(stderr.contains("malformed-events.json"), "stderr: {stderr}");
}
