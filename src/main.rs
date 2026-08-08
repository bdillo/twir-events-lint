use clap::Parser;
use log::{error, info};
use std::fs;
use twir_events_lint::{
    args::Args, events::EventsByRegion, linter::EventLinter, reader::EventsSection,
};

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let log_level = if args.debug() {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    simple_logger::init_with_level(log_level).expect("failed to init logger");

    info!("reading file '{}'", args.draft().display());
    let content = fs::read_to_string(args.draft())?;
    let events_section = EventsSection::find(&content)?;

    let mut linter = EventLinter::new(args.error_limit());
    if let Err(e) = linter.lint(events_section.reader()) {
        error!("{}", e);
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
