// ok what are we doing
// need to capture events from the newsletter
// maybe hash map w/ region as key, vec of events
// read in the same thing from the python script
// compare events, deduplicate, insert in correct order

use std::{collections::HashMap, fs};

use clap::Parser;
use log::{debug, info};
use twir_events_lint::{
    args::MergerArgs,
    constants::REGIONS,
    event_line_types::{EventDateLocationGroup, EventLineType, EventNameUrl},
    lint::LinterState,
    twir_reader::{TwirLineError, TwirReader},
};

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
struct TwirEvent {
    date_location_group: EventDateLocationGroup,
    event_name: Vec<EventNameUrl>,
}

// TODO: probably push these fmts into EventDateLocationGroup and EventNameUrl?
impl std::fmt::Display for TwirEvent {
    // example outputs
    // * 2024-10-24 | Virtual | [Women in Rust](https://www.meetup.com/women-in-rust/)
    //     * [**Part 4 of 4 - Hackathon Showcase: Final Projects and Presentations**](https://www.meetup.com/women-in-rust/events/303213835/)
    // * 2024-10-24 | Virtual (Berlin, DE) | [OpenTechSchool Berlin](https://berline.rs/) + [Rust Berlin](https://www.meetup.com/rust-berlin/)
    //     * [**Rust Hack and Learn**](https://meet.jit.si/RustHackAndLearnBerlin) | [**Mirror: Rust Hack n Learn Meetup**](https://www.meetup.com/rust-berlin/events/298633271/)
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut formatted = String::new();

        formatted.push_str("* ");
        formatted.push_str(
            &self
                .date_location_group
                .date()
                .format("%Y-%m-%d")
                .to_string(),
        );
        formatted.push_str(" | ");
        formatted.push_str(self.date_location_group.location());
        formatted.push_str(" | ");

        let organizers_str = self
            .date_location_group
            .organizers()
            .iter()
            .map(|(name, url)| format!("[{}]({})", name, url))
            .collect::<Vec<String>>()
            .join(" + ");

        formatted.push_str(&organizers_str);
        formatted.push('\n');

        formatted.push_str("    * ");
        let event_str = self
            .event_name
            .iter()
            .map(|e| format!("[{}]({})", e.name(), e.url()))
            .collect::<Vec<String>>()
            .join(" | ");
        formatted.push_str(&event_str);

        f.write_str(&formatted)
    }
}

impl TwirEvent {
    // we need a unique key to identify events. we want to be able to:
    // - update event titles if they change from week to week (new event name, same event link)
    // - update event date if it is rescheduled from week to week (same event link, new time)
    // - remove event if it was cancelled (not sure how we'd do this exactly, would probably have to check meetup directly)
    //
    // what are we gonna do with the key? i guess when reading the new events from the python script, if we see the same key,
    // we know we need to do something to merge it.
    fn event_key(&self) -> Vec<String> {
        self.event_name
            .iter()
            .map(|e| e.url().to_string())
            .collect()
    }
}

fn collect_events(reader: TwirReader) -> Result<HashMap<String, Vec<TwirEvent>>, TwirLineError> {
    let mut results: HashMap<String, Vec<TwirEvent>> = HashMap::new();
    let mut state = LinterState::ExpectingRegionalHeader;
    let mut in_event_section = false;
    let mut current_region = String::new();

    let mut event_date_location: Option<EventDateLocationGroup> = None;

    // TODO: clean up errors
    for line in reader {
        let line = line?;
        debug!("read line:\n{}", line);
        match line.line_type() {
            EventLineType::Newline => {
                if !in_event_section {
                    continue;
                }

                if state != LinterState::ExpectingEventDateLocationGroupLink {
                    panic!("wrong state")
                }
            }
            EventLineType::Header(maybe_region) => {
                if let Some(region) = maybe_region {
                    if !in_event_section {
                        in_event_section = true;
                    }

                    current_region = region.clone();
                    state = LinterState::ExpectingEventDateLocationGroupLink;
                } else if in_event_section {
                    panic!("expected region header")
                }
            }
            EventLineType::EventDate(event_date) => {
                if state != LinterState::ExpectingEventDateLocationGroupLink {
                    panic!("wrong state")
                }
                event_date_location = Some(event_date.clone());
                state = LinterState::ExpectingEventNameLink
            }
            EventLineType::EventInfo(event_name_urls) => {
                if state != LinterState::ExpectingEventNameLink {
                    panic!("wrong state")
                }
                // event_name = Some(event_name_urls.clone());
                state = LinterState::ExpectingEventDateLocationGroupLink;

                let edl = event_date_location.take().unwrap();
                let name = event_name_urls.clone();
                if current_region.is_empty() {
                    panic!("region not set")
                }

                let event = TwirEvent {
                    date_location_group: edl,
                    event_name: name,
                };

                results
                    .entry(current_region.clone())
                    .or_default()
                    .push(event);
            }
            EventLineType::EndEventSection => {
                if !in_event_section {
                    panic!("couldn't find event section")
                }
                break;
            }
            _ => {
                if !in_event_section {
                    continue;
                } else {
                    panic!("unsupported line:\n{}", line)
                }
            }
        }
    }

    Ok(results)
}

fn merge_events(draft_events: &[TwirEvent], new_events: &[TwirEvent]) -> Vec<TwirEvent> {
    let mut events_map: HashMap<Vec<String>, TwirEvent> = HashMap::new();

    for draft_event in draft_events {
        events_map.insert(draft_event.event_key(), draft_event.clone());
    }

    for new_event in new_events {
        let new_event_key = new_event.event_key();

        if events_map.contains_key(&new_event_key) {
            // if we have a match, it means we have the same event and need to take some action
            let draft_event = events_map.get_mut(&new_event_key).unwrap();

            if draft_event == new_event {
                // event hasn't changed, continue on
                debug!("keeping unchanged event {:?}", new_event_key);
                continue;
            } else {
                // something has been updated - use the newer version of the event
                debug!("updated event {:?}", new_event_key);
                let _ = std::mem::replace(draft_event, new_event.clone());
            }
        } else {
            debug!("found new event: {:?}", new_event_key);
            events_map.insert(new_event_key, new_event.clone());
        }
    }
    let mut updated_events: Vec<TwirEvent> = Vec::new();
    for event in events_map.into_values() {
        updated_events.push(event);
    }

    updated_events
}

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

    let draft_events = collect_events(draft_reader).expect("failed to collect draft events");
    let new_events = collect_events(new_events_reader).expect("failed to collect new events");

    for region in REGIONS {
        // check if the region exists in draft events, new events, both, or neither
        let region_draft_events = draft_events.get(region);
        let region_new_events = new_events.get(region);

        // if one has events in a region and the other doesn't, just take all events from the one that has the region
        // no merging needed
        if region_draft_events.is_none() && region_new_events.is_none() {
            continue;
        }
        println!("{}", region);
        if region_draft_events.is_some() && region_new_events.is_none() {
            for event in region_draft_events.unwrap() {
                println!("{}", event);
            }
            println!();
            continue;
        }
        if region_draft_events.is_none() && region_new_events.is_some() {
            for event in region_new_events.unwrap() {
                println!("{}", event);
            }
            println!();
            continue;
        }

        let mut merged = merge_events(region_draft_events.unwrap(), region_new_events.unwrap());
        merged.sort();

        for event in merged {
            println!("{}", event);
        }
        println!();
    }
}
