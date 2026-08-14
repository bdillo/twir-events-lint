use chrono::NaiveDate;
use clap::Parser;
use log::{error, info, warn};
use std::{fs, path::Path};
use twir_events_lint::{
    args::Args,
    collect,
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

fn log_diagnostics(linter: &EventLinter) {
    for diagnostic in linter.diagnostics() {
        error!("{diagnostic}");
    }
}

fn read_collected_events(path: &Path) -> anyhow::Result<EventsByRegion> {
    info!("reading new events file '{}'", path.display());
    let content = fs::read_to_string(path)
        .map_err(|error| anyhow::anyhow!("failed to read {}: {}", path.display(), error))?;
    let collected: CollectedEventsByRegion = serde_json::from_str(&content)
        .map_err(|error| anyhow::anyhow!("failed to parse {}: {}", path.display(), error))?;
    EventsByRegion::try_from(collected).map_err(|error| {
        anyhow::anyhow!(
            "failed to convert events from {}: {}",
            path.display(),
            error
        )
    })
}

fn collect_incoming_events(
    args: &Args,
    range_start: NaiveDate,
    range_end: NaiveDate,
) -> anyhow::Result<Option<EventsByRegion>> {
    let mut incoming = EventsByRegion::new();
    let mut has_source = false;

    if let Some(sources_path) = args.event_sources() {
        info!("collecting events using '{}'", sources_path.display());
        let collection = collect::collect(sources_path, range_start, range_end)?;
        for warning in collection.warnings {
            warn!("{warning}");
        }
        incoming = incoming.merge(&collection.events);
        has_source = true;
    }

    if let Some(new_events_file) = args.new_events_file() {
        incoming = incoming.merge(&read_collected_events(new_events_file)?);
        has_source = true;
    }

    Ok(has_source.then_some(incoming))
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let log_level = if args.debug() {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    simple_logger::init_with_level(log_level).expect("failed to init logger");

    if args.in_place() && args.new_events_file().is_none() && args.event_sources().is_none() {
        anyhow::bail!("--in-place requires --new-events-file or --event-sources");
    }

    info!("reading file '{}'", args.draft().display());
    let original = fs::read_to_string(args.draft())?;
    let mut updated = original.clone();
    let (mut linter, lint_result) = lint_document(&updated, args.error_limit())?;
    let mut fix_count = 0;

    if let Err(error) = lint_result {
        if !args.fix() {
            log_diagnostics(&linter);
            error!("{}", error);
            std::process::exit(1);
        }
        let edits = linter.safe_edits(&updated)?;
        if edits.is_empty() {
            log_diagnostics(&linter);
            error!("{}", error);
            std::process::exit(1);
        }

        fix_count = edits.len();
        updated = apply_edits(&updated, &edits)?;
        let linted = lint_document(&updated, args.error_limit())?;
        linter = linted.0;
        if let Err(error) = linted.1 {
            log_diagnostics(&linter);
            error!("{}", error);
            std::process::exit(1);
        }
    }

    let (range_start, range_end) = linter.newsletter_range().ok_or_else(|| {
        anyhow::anyhow!("newsletter date range unavailable after successful lint")
    })?;
    if let Some(incoming) = collect_incoming_events(&args, range_start, range_end)? {
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
        let (candidate_linter, candidate_result) = lint_document(&candidate, args.error_limit())?;
        if let Err(error) = candidate_result {
            log_diagnostics(&candidate_linter);
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
