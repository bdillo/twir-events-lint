use clap::Parser;
use log::{error, info};
use std::fs;
use twir_events_lint::{args::Args, lint::EventSectionLinter, twir_reader::TwirReader};

fn main() {
    let args = Args::parse();

    let log_level = if args.debug() {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    simple_logger::init_with_level(log_level).expect("Failed to init logger!");

    info!("Reading file '{}'", args.file().display());
    let md_contents = fs::read_to_string(args.file()).unwrap();
    let reader = TwirReader::new(&md_contents);

    let mut event_linter = EventSectionLinter::new(args.error_limit());
    match event_linter.lint(reader) {
        Ok(_) => info!("LGTM!"),
        Err(e) => error!("{}", e),
    }
}
