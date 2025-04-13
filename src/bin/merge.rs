use std::fs;

use clap::Parser;
use log::{debug, info};
use twir_events_lint::{
    args::MergerArgs,
    constants::REGIONS,
    merger::{collect_events, merge_events, TwirEvent},
    twir_reader::TwirReader,
};

fn main() {
    let args = MergerArgs::parse();

    let log_level = if args.debug() {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    simple_logger::init_with_level(log_level).expect("failed to init logger!");

    info!("reading draft from '{}'", args.file().display());
    info!(
        "reading new events from '{}'",
        args.new_events_file().display()
    );

    let draft_contents = fs::read_to_string(args.file()).unwrap();
    let draft_reader = TwirReader::new(&draft_contents);

    let new_events_contents = fs::read_to_string(args.new_events_file()).unwrap();
    let new_events_reader = TwirReader::new(&new_events_contents);

    let (draft_events, date_range) =
        collect_events(draft_reader).expect("failed to collect draft events");
    let (new_events, _) = collect_events(new_events_reader).expect("failed to collect new events");

    let date_range = date_range.expect("unable to find date range in draft");

    // TODO: print out everything before/after the draft section, rather than just the event section (then no need to copy/paste)
    for region in REGIONS {
        let mut events: Vec<TwirEvent> = Vec::new();
        // check if the region exists in draft events, new events, both, or neither
        let region_draft_events = draft_events.get(region);
        let region_new_events = new_events.get(region);

        // if one has events in a region and the other doesn't, just take all events from the one that has the region
        // no merging needed
        if region_draft_events.is_none() && region_new_events.is_none() {
            continue;
        }

        if region_draft_events.is_some() && region_new_events.is_none() {
            for event in region_draft_events.unwrap() {
                events.push(event.clone());
            }
        } else if region_draft_events.is_none() && region_new_events.is_some() {
            for event in region_new_events.unwrap() {
                events.push(event.clone())
            }
        } else {
            let merged = merge_events(region_draft_events.unwrap(), region_new_events.unwrap());
            for event in merged {
                events.push(event);
            }
        }

        events.sort();
        let mut region_printed = false;

        for event in events {
            let event_date = event.date_location_group().date();
            if event_date < date_range.0 || event_date > date_range.1 {
                debug!("skipping event, out of date range {:?}", event.event_key());
                continue;
            }

            // don't print the region until we have at least one event, so we don't print empty region headers
            if !region_printed {
                println!("### {}", region);
                region_printed = true;
            }

            println!("{}", event);
        }
        println!();
    }
}
