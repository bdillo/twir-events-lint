use std::{collections::HashMap, fs};

use chrono::NaiveDate;
use clap::Parser;
use log::{debug, info};
use twir_events_lint::{
    args::MergerArgs,
    constants::REGIONS,
    event_line_types::{EventDateLocationGroup, EventLineType, EventNameUrl},
    lint::LinterState,
    twir_reader::{TwirLineError, TwirReader},
};

// TODO:
// strip events outside of date range
// make it so we insert all the new events into the draft output

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

fn collect_events(
    reader: TwirReader,
) -> Result<
    (
        HashMap<String, Vec<TwirEvent>>,
        Option<(NaiveDate, NaiveDate)>,
    ),
    TwirLineError,
> {
    let mut results: HashMap<String, Vec<TwirEvent>> = HashMap::new();
    let mut state = LinterState::ExpectingRegionalHeader;

    let mut in_event_section = false;
    let mut current_region = String::new();
    let mut date_range: Option<(NaiveDate, NaiveDate)> = None;

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
                state = LinterState::ExpectingEventDateLocationGroupLink;

                let date_location = event_date_location.take().unwrap();
                let name = event_name_urls.clone();
                if current_region.is_empty() {
                    panic!("region not set")
                }

                let event = TwirEvent {
                    date_location_group: date_location,
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
            EventLineType::EventsDateRange(start_date, end_date) => {
                if date_range.is_some() {
                    panic!("already set date range, can't set again")
                } else {
                    date_range = Some((*start_date, *end_date))
                }
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

    Ok((results, date_range))
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

    let (draft_events, date_range) =
        collect_events(draft_reader).expect("failed to collect draft events");
    let (new_events, _) = collect_events(new_events_reader).expect("failed to collect new events");

    let date_range = date_range.expect("unable to find date range in draft");

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
        println!("### {}", region);
        for event in events {
            let event_date = event.date_location_group.date();
            if event_date < date_range.0 || event_date > date_range.1 {
                debug!("skipping event, out of date range {:?}", event.event_key());
                continue;
            }
            println!("{}", event);
        }
        println!();
    }
}
