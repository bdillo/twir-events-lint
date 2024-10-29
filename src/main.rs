use std::{error::Error, fs};

use clap::Parser;
use log::{error, info};
use twir_events_lint::{args::Args, lint::EventSectionLinter};

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let log_level = if args.debug() {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    simple_logger::init_with_level(log_level).expect("Failed to init logger!");

    info!("Reading file '{}'", args.file().display());
    let md = fs::read_to_string(args.file())?;

    let mut event_linter = EventSectionLinter::default();
    match event_linter.lint(&md) {
        Ok(_) => info!("LGTM!"),
        // TODO: switch from debug to display when we make good error messages
        Err(e) => error!("{:?}", e),
    }

    Ok(())
}
