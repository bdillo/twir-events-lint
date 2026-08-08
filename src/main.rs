use clap::Parser;
use log::{error, info};
use std::fs;
use twir_events_lint::{
    args::Args,
    edit::{apply_edits, replace_file},
    events::EventsByRegion,
    linter::{EventLinter, LintError},
    reader::EventsSection,
};

fn lint_document(
    content: &str,
    error_limit: u16,
) -> anyhow::Result<(EventLinter, Result<(), LintError>)> {
    let events_section = EventsSection::find(content)?;
    let mut linter = EventLinter::new(error_limit);
    let result = linter.lint(events_section.reader());
    Ok((linter, result))
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let log_level = if args.debug() {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    simple_logger::init_with_level(log_level).expect("failed to init logger");

    info!("reading file '{}'", args.draft().display());
    let mut content = fs::read_to_string(args.draft())?;
    let (mut linter, mut lint_result) = lint_document(&content, args.error_limit())?;

    if args.fix() && !linter.safe_edits().is_empty() {
        let edit_count = linter.safe_edits().len();
        let updated = apply_edits(&content, linter.safe_edits())?;
        replace_file(args.draft(), &updated)?;
        info!("applied {edit_count} safe edit(s)");

        content = updated;
        (linter, lint_result) = lint_document(&content, args.error_limit())?;
    }

    if let Err(error) = lint_result {
        error!("{}", error);
        std::process::exit(1);
    }
    info!("lgtm!");

    if let Some(new_events_file) = args.new_events_file() {
        info!("reading new events file '{}'", new_events_file.display());
        let content = fs::read_to_string(new_events_file)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", new_events_file.display(), e))?;
        let new_events: EventsByRegion = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("failed to parse {}: {}", new_events_file.display(), e))?;

        let merged = linter.events().merge(&new_events);
        println!("{merged}");
    }

    Ok(())
}
