use clap::Parser;
use log::{error, info};
use std::fs;
use twir_events_lint::{args::Args, events::EventsByRegion, linter::EventLinter, reader::Reader};

fn main() {
    let args = Args::parse();

    let log_level = if args.debug() {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    simple_logger::init_with_level(log_level).expect("failed to init logger");

    info!("reading file '{}'", args.draft().display());
    let md_contents = fs::read_to_string(args.draft()).unwrap();
    let reader = Reader::new(&md_contents);

    let mut linter = EventLinter::new(args.error_limit());
    match linter.lint(reader) {
        Ok(_) => info!("lgtm!"),
        Err(e) => error!("{}", e),
    }

    if let Some(new_events_file) = args.new_events_file() {
        info!("reading new events file '{}", new_events_file.display());
        let new_events: EventsByRegion =
            serde_json::from_str(&fs::read_to_string(new_events_file).unwrap()).unwrap();

        let merged = linter.events().merge(&new_events);
        println!("{merged}");
    };
}
