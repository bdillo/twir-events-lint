use clap::Parser;
use log::{error, info};
use std::fs;
use twir_events_lint::{args::LinterArgs, linter::EventLinter, reader::Reader};

fn main() {
    let args = LinterArgs::parse();

    let log_level = if args.debug() {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    simple_logger::init_with_level(log_level).expect("failed to init logger!");

    info!("reading file '{}'", args.file().display());
    let md_contents = fs::read_to_string(args.file()).unwrap();
    let reader = Reader::new(&md_contents);

    let mut event_linter = EventLinter::new(args.error_limit());
    match event_linter.lint(reader) {
        Ok(_) => info!("lgtm!"),
        Err(e) => error!("{}", e),
    }
}
