use std::collections::HashMap;

use crate::{
    line_types::{EventDateLocationGroup, EventLineType, EventNameUrl},
    linter::LinterState,
    reader::{Line, LineError, Reader},
};
use chrono::NaiveDate;
use log::debug;

// TODO:
// make it so we insert all the new events into the draft output

#[derive(Debug)]
pub enum EventMergerError<'a> {
    BadStateTransition(Line<'a>),
    LineParse(LineError),
}

impl<'a> std::fmt::Display for EventMergerError<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let out = match self {
            Self::BadStateTransition(line) => {
                format!("bad state transition on line: {}", line)
            }
            Self::LineParse(twir_line_error) => twir_line_error.to_string(),
        };
        f.write_str(&out)
    }
}

impl<'a> From<LineError> for EventMergerError<'a> {
    fn from(value: LineError) -> Self {
        Self::LineParse(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
pub struct TwirEvent {
    date_location_group: EventDateLocationGroup,
    event_name: Vec<EventNameUrl>,
}

impl TwirEvent {
    pub fn date_location_group(&self) -> &EventDateLocationGroup {
        &self.date_location_group
    }

    pub fn event_name(&self) -> &[EventNameUrl] {
        &self.event_name
    }
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
    pub fn event_key(&self) -> Vec<String> {
        self.event_name
            .iter()
            .map(|e| e.url().to_string())
            .collect()
    }
}

type EventsByRegion = HashMap<String, Vec<TwirEvent>>;

pub fn collect_events(
    reader: Reader,
) -> Result<(EventsByRegion, Option<(NaiveDate, NaiveDate)>), EventMergerError> {
    let mut results: HashMap<String, Vec<TwirEvent>> = HashMap::new();
    let mut state = LinterState::ExpectingRegion;

    let mut in_event_section = false;
    // TODO: move to options?
    let mut current_region = String::new();
    let mut date_range: Option<(NaiveDate, NaiveDate)> = None;

    let mut event_date_location: Option<EventDateLocationGroup> = None;

    // TODO: clean up errors
    for line in reader {
        let line = line?;
        debug!("read line:\n{}", line);
        match line.get_line_type() {
            EventLineType::Newline => {
                if !in_event_section {
                    continue;
                }

                if state != LinterState::ExpectingEventDate {
                    return Err(EventMergerError::BadStateTransition(line.to_owned()));
                }
            }
            EventLineType::Header(maybe_region) => {
                if let Some(region) = maybe_region {
                    if !in_event_section {
                        in_event_section = true;
                    }

                    current_region = region.clone();
                    state = LinterState::ExpectingEventDate;
                } else if in_event_section {
                    panic!("expected region header")
                }
            }
            EventLineType::EventDate(event_date) => {
                if state != LinterState::ExpectingEventDate {
                    return Err(EventMergerError::BadStateTransition(line.to_owned()));
                }
                event_date_location = Some(event_date.clone());
                state = LinterState::ExpectingEventInfo
            }
            EventLineType::EventInfo(event_name_urls) => {
                if state != LinterState::ExpectingEventInfo {
                    return Err(EventMergerError::BadStateTransition(line.to_owned()));
                }
                state = LinterState::ExpectingEventDate;

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

pub fn merge_events(draft_events: &[TwirEvent], new_events: &[TwirEvent]) -> Vec<TwirEvent> {
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

mod test {
    use std::{fs, path::Path};

    use super::*;

    fn read_test<P: AsRef<Path>>(path: P) -> Vec<TwirEvent> {
        let md = fs::read_to_string(path).expect("failed to read file");
        let reader = Reader::new(&md);
        let events = collect_events(reader).expect("failed to read events from draft");
        println!("{:?}", events);
        events
            .0
            .get("Virtual")
            .expect("failed to get virtual events from draft")
            .clone()
    }

    #[test]
    fn test_merge() {
        let draft = read_test("./test/merge/draft.md");
        let updated = read_test("./test/merge/updated.md");

        let mut merged = merge_events(&draft, &updated);
        merged.sort();

        let expected = read_test("./test/merge/expected.md");
        assert_eq!(merged, expected);
    }
}
