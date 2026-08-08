use clap::Parser;
use log::{error, info, warn};
use std::fs;
use twir_events_lint::{
    args::Args,
    edit::{TextEdit, apply_edits, replace_file},
    events::{CollectedEventsByRegion, EventsByRegion},
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
    let original = fs::read_to_string(args.draft())?;
    let mut updated = original.clone();
    let (mut linter, lint_result) = lint_document(&updated, args.error_limit())?;
    let mut fix_count = 0;

    if let Err(error) = lint_result {
        if !args.fix() || linter.safe_edits().is_empty() {
            error!("{}", error);
            std::process::exit(1);
        }

        fix_count = linter.safe_edits().len();
        updated = apply_edits(&updated, linter.safe_edits())?;
        let linted = lint_document(&updated, args.error_limit())?;
        linter = linted.0;
        if let Err(error) = linted.1 {
            error!("{}", error);
            std::process::exit(1);
        }
    }

    if let Some(new_events_file) = args.new_events_file() {
        info!("reading new events file '{}'", new_events_file.display());
        let content = fs::read_to_string(new_events_file)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", new_events_file.display(), e))?;
        let collected: CollectedEventsByRegion = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("failed to parse {}: {}", new_events_file.display(), e))?;
        let incoming = EventsByRegion::try_from(collected).map_err(|e| {
            anyhow::anyhow!(
                "failed to convert events from {}: {}",
                new_events_file.display(),
                e
            )
        })?;

        let (range_start, range_end) = linter.newsletter_range().ok_or_else(|| {
            anyhow::anyhow!("newsletter date range unavailable after successful lint")
        })?;
        let (incoming, dropped) = incoming.partition_by_date_range(range_start, range_end);
        for (region, event) in dropped {
            let names = event
                .events()
                .iter()
                .map(|event| event.name())
                .collect::<Vec<_>>()
                .join(", ");
            warn!(
                "dropping out-of-range incoming event '{names}' in {region}: {}",
                event.overview()
            );
        }

        let merged = linter.events().merge(&incoming);
        let listings_span = linter.event_listings_span().ok_or_else(|| {
            anyhow::anyhow!("event listing source span unavailable after successful lint")
        })?;
        let candidate = apply_edits(
            &updated,
            &[TextEdit::new(listings_span, merged.to_string())],
        )?;
        let (_, candidate_result) = lint_document(&candidate, args.error_limit())?;
        if let Err(error) = candidate_result {
            error!("merged document is invalid: {error}");
            std::process::exit(1);
        }

        if args.in_place() {
            updated = candidate;
        } else {
            print!("{merged}");
        }
    }

    if updated != original {
        replace_file(args.draft(), &updated)?;
        if fix_count > 0 {
            info!("applied {fix_count} safe edit(s)");
        }
        if args.in_place() {
            info!("updated merged events in '{}'", args.draft().display());
        }
    }

    info!("lgtm!");
    Ok(())
}
