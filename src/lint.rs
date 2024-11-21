use std::fmt;

use chrono::{NaiveDate, ParseError};
use log::{debug, error};
use url::Url;

use crate::{
    constants::*,
    event_line_types::{EventDateLocation, EventLineType},
};

// TODO:
// - lint for empty regions
// - clean up errors and error messages
// - tests
// - add tools for adding new events
// - check for duplicated links
// - make sure each location in virtual section starts with "virtual"

/// An error linting - this error should provide enough information by itself to be useful to a user (one would hope)
// TODO: probably split this into linter logic errors (like invalid state transitions) and parsing/validation errors
#[derive(Debug, PartialEq, Eq)]
pub enum LintError {
    InvalidStateChange {
        from: String,
    },
    UnexpectedDateRange,
    UnexpectedLineType {
        linter_state: String,
        line_type: String,
        expected_line_types: Vec<String>,
    },
    EventOutOfDateRange {
        event_date: NaiveDate,
        date_range: (NaiveDate, NaiveDate),
    },
    EventOutOfOrder {
        event_date: NaiveDate,
        event_location: String,
        previous_event_date: NaiveDate,
        previous_event_location: String,
    },
    DateRangeNotSet,
    RegexError {
        regex_string: String,
    },
    DateParseError {
        chrono_error: ParseError,
    },
    // TODO: generic error - clean this up later
    ParseError,
    // TODO: make this useful
    UnexpectedEnd,
    /// Top level error to return to main if we find any errors
    LintFailed,
    /// An invalid url in our events
    InvalidUrl(url::ParseError),
    /// A region header (Virtual, Europe, etc) we do not recognize
    UnknownRegion(String),
    /// URL contains a tracker that we want to strip out
    UrlContainsTracker(Url),
    /// Invalid format for a link label, e.g. [link label](https://mylink.test)
    InvalidLinkLabel(String),
}

impl fmt::Display for LintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let error_msg = match self {
            Self::InvalidStateChange { from } => {
                format!("Invalid state change from state {}", from)
            }
            Self::UnexpectedDateRange => {
                "We read two expected date ranges! This is almost certainly a linter bug".to_owned()
            }
            Self::UnexpectedLineType {
                linter_state,
                line_type,
                expected_line_types,
            } => {
                let types = expected_line_types.join(" ");
                let trimmed = types.trim();
                format!(
                    "Linter in state '{}'\nExpected line type(s): '{}'\nFound line type '{}'",
                    linter_state, line_type, trimmed
                )
            }
            Self::EventOutOfDateRange {
                event_date,
                date_range,
            } => {
                format!(
                    "Event date '{}' does not fall within newsletter date range '{} - {}'",
                    event_date, date_range.0, date_range.1
                )
            }
            Self::EventOutOfOrder {
                event_date,
                event_location,
                previous_event_date,
                previous_event_location,
            } => {
                format!(
                    "Event date '{}' and location '{}' should be after previous event date '{}' and location '{}'",
                    event_date, event_location, previous_event_date, previous_event_location
                )
            }
            Self::DateRangeNotSet => {
                "Found an event date but we haven't set the date range to compare it to".to_owned()
            }
            Self::RegexError { regex_string } => {
                format!("Line does not match regex '{}'", regex_string)
            }
            Self::DateParseError { chrono_error } => {
                format!("Error parsing date: '{}'", chrono_error)
            }
            Self::ParseError => "Parse error".to_owned(), // TODO: is this needed?
            Self::UnexpectedEnd => "Reached unexpected end of file".to_owned(),
            Self::LintFailed => "Lint failed! See above for error details".to_owned(),
            Self::InvalidUrl(e) => format!("URL parsing error: '{}'", e),
            Self::UnknownRegion(region) => format!(
                "Found unknown region: '{}'\nExpected one of '{:?}'",
                region, REGIONS
            ),
            Self::UrlContainsTracker(url) => format!("URL '{}' contains a tracker", url),
            Self::InvalidLinkLabel(label) => format!("Link label '{}' is invalid", label),
        };

        write!(f, "{}", error_msg)
    }
}

impl std::error::Error for LintError {}

/// Overall state of the linter, keeps track of what "section" we are in
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinterState {
    /// Not yet in event section
    PreEvents,
    /// Expecting our event date range, which we will use to verify event fall within our expected range
    ExpectingDateRange,
    /// Expecting a regional event section (e.g. Virtual, Asia, Europe, etc)
    ExpectingRegionalHeader,
    /// Expecting a date, location, and group event line
    ExpectingEventDateLocationGroupLink,
    /// Expecting an event name and event link
    ExpectingEventNameLink,
    /// We have finished reading the entire event section
    Done,
}

impl LinterState {
    fn new() -> Self {
        Self::PreEvents
    }

    fn next(&self) -> Result<Self, LintError> {
        // TODO: does this really make sense? There is branching in the states so this should be modeled differently
        match self {
            Self::PreEvents => Ok(Self::ExpectingDateRange),
            Self::ExpectingDateRange => Ok(Self::ExpectingRegionalHeader),
            Self::ExpectingRegionalHeader => Ok(Self::ExpectingEventDateLocationGroupLink),
            Self::ExpectingEventDateLocationGroupLink => Ok(Self::ExpectingEventNameLink),
            Self::ExpectingEventNameLink => Ok(Self::ExpectingEventDateLocationGroupLink),
            _ => Err(LintError::InvalidStateChange {
                from: self.to_string(),
            }),
        }
    }

    fn finish_regional_section(&self) -> Result<Self, LintError> {
        match self {
            Self::ExpectingEventDateLocationGroupLink => Ok(Self::ExpectingRegionalHeader),
            _ => Err(LintError::InvalidStateChange {
                from: self.to_string(),
            }),
        }
    }

    fn finish(&self) -> Result<Self, LintError> {
        match self {
            Self::ExpectingRegionalHeader => Ok(Self::Done),
            _ => Err(LintError::InvalidStateChange {
                from: self.to_string(),
            }),
        }
    }
}

impl fmt::Display for LinterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::PreEvents => "PreEvents",
            Self::ExpectingDateRange => "ExpectingDateRange",
            Self::ExpectingRegionalHeader => "ExpectingRegionalHeader",
            Self::ExpectingEventDateLocationGroupLink => "ExpectingEventDateLocationGroupLink",
            Self::ExpectingEventNameLink => "ExpectingEventNameLink",
            Self::Done => "Done",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug)]
pub struct EventSectionLinter {
    /// Our current state of the linter
    linter_state: LinterState,
    /// Date range - this is unknown until we reach the date range line. Used for validating dates fall within the given range
    event_date_range: Option<(NaiveDate, NaiveDate)>,
    /// Region we are in
    current_region: Option<String>,
    /// The last event in our current region. Used to make sure we have our events properly sorted by date and location name
    previous_event: Option<EventDateLocation>,
    // TODO: keep track of newlines here, like in a counter? So we can lint for unexpected newlines between sections
}

impl Default for EventSectionLinter {
    fn default() -> Self {
        Self {
            linter_state: LinterState::new(),
            event_date_range: None,
            current_region: None,
            previous_event: None,
        }
    }
}

impl EventSectionLinter {
    pub fn lint(&mut self, md: &str) -> Result<(), LintError> {
        let lines: Vec<&str> = md.lines().collect();
        let mut error_count = 0;

        for (i, line) in lines.iter().enumerate() {
            match self.read_line(i, line) {
                Ok(_) => (),
                Err(e) => {
                    // we don't care about any errors before the event section, which we expect a lot of because it's
                    // not modeled in our linter
                    // TODO: clean this up, we are just assuming all headers ("###") are regions, which is the source of the errors
                    if self.linter_state == LinterState::PreEvents {
                        continue;
                    }

                    error!(
                        "Linter Error:\n{}\nCaused by line #{}: '{}'\n",
                        e,
                        i + 1,
                        line
                    );

                    // attempt to continue to parse, this could print out a bunch of errors in some cases
                    self.linter_state = self.linter_state.next()?;

                    error_count += 1;

                    // if we reach this many errors something has probably gone very wrong, so just exit early
                    // rather than overwhelming the output with more error messages
                    // TODO: make this a configurable arg
                    if error_count == 10 {
                        error!("Reached our maximum error limit, bailing");
                        return Err(LintError::LintFailed);
                    }
                }
            }
        }

        if self.linter_state != LinterState::Done {
            return Err(LintError::UnexpectedEnd);
        }

        if error_count > 0 {
            Err(LintError::LintFailed)
        } else {
            Ok(())
        }
    }

    fn read_line(&mut self, line_num: usize, line: &str) -> Result<(), LintError> {
        let line_type = line.parse::<EventLineType>()?;
        debug!(
            "In state {}, parsed line #{} '{}' as '{:?}'",
            self.linter_state.to_string(),
            line_num,
            line,
            line_type
        );

        match &self.linter_state {
            LinterState::PreEvents => self.handle_pre_events(line_type),
            LinterState::ExpectingDateRange => self.handle_expected_date_range(line_type),
            LinterState::ExpectingRegionalHeader => {
                self.handle_expecting_regional_header(line_type)
            }
            LinterState::ExpectingEventDateLocationGroupLink => {
                self.handle_expecting_event_date_location_group_link(line_type)
            }
            LinterState::ExpectingEventNameLink => self.handle_expecting_event_name_link(line_type),
            LinterState::Done => Ok(()),
        }
    }

    /// Handler before we are in the events section. Accepts all lines and just continues until we hit the event section
    fn handle_pre_events(&mut self, line_type: EventLineType) -> Result<(), LintError> {
        match line_type {
            EventLineType::StartEventSection => {
                self.linter_state = self.linter_state.next()?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Handler to run when we are expecting to receive a date range line
    fn handle_expected_date_range(&mut self, line_type: EventLineType) -> Result<(), LintError> {
        match line_type {
            EventLineType::Newline => Ok(()),
            EventLineType::EventsDateRange(start_date, end_date) => {
                if self.event_date_range.is_none() {
                    self.event_date_range = Some((start_date, end_date));
                    self.linter_state = self.linter_state.next()?;
                    Ok(())
                } else {
                    Err(LintError::UnexpectedDateRange)
                }
            }
            _ => Err(LintError::UnexpectedLineType {
                linter_state: self.linter_state.to_string(),
                line_type: line_type.to_string(),
                expected_line_types: vec![
                    NEWLINE_TYPE.to_string(),
                    EVENTS_DATE_RANGE_TYPE.to_string(),
                ],
            }),
        }
    }

    fn handle_expecting_regional_header(
        &mut self,
        line_type: EventLineType,
    ) -> Result<(), LintError> {
        match line_type {
            EventLineType::Newline => Ok(()),
            EventLineType::EventRegionHeader(region) => {
                // TODO: check if region is already set?
                self.current_region = Some(region);
                self.linter_state = self.linter_state.next()?;
                Ok(())
            }
            EventLineType::EndEventSection => {
                self.linter_state = self.linter_state.finish()?;
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                linter_state: self.linter_state.to_string(),
                line_type: line_type.to_string(),
                expected_line_types: vec![
                    NEWLINE_TYPE.to_string(),
                    EVENT_REGION_HEADER_TYPE.to_string(),
                    END_EVENTS_SECTION.to_string(),
                ],
            }),
        }
    }

    fn handle_expecting_event_date_location_group_link(
        &mut self,
        line_type: EventLineType,
    ) -> Result<(), LintError> {
        match line_type {
            EventLineType::EventDateLocationGroup(event_date_location) => {
                // validate event is within date range
                if let Some(date_range) = &self.event_date_range {
                    if (*event_date_location.date() < date_range.0)
                        || (*event_date_location.date() > date_range.1)
                    {
                        return Err(LintError::EventOutOfDateRange {
                            event_date: *event_date_location.date(),
                            date_range: *date_range,
                        });
                    }
                // if we don't have the date range set, we are in an unexpected state
                } else {
                    return Err(LintError::DateRangeNotSet);
                }

                // if there is a previous event, compare to make sure our current one is later than the previous one
                if let Some(previous_event) = &self.previous_event {
                    // TODO: make sure this comparison is correct
                    // if event_date_location > *previous_event {
                    if event_date_location < *previous_event {
                        return Err(LintError::EventOutOfOrder {
                            event_date: *event_date_location.date(),
                            event_location: event_date_location.location().to_owned(),
                            previous_event_date: *previous_event.date(),
                            previous_event_location: previous_event.location().to_owned(),
                        });
                    }
                }

                // and save our previous event so we can compare it when looking at the next event
                self.previous_event = Some(event_date_location);
                self.linter_state = self.linter_state.next()?;

                Ok(())
            }
            // If we hit a newline it should mean that we are done with a given regional section (Virtual, Asia, etc)
            EventLineType::Newline => {
                self.linter_state = self.linter_state.finish_regional_section()?;
                // and reset our previous event to None, ordering is only internal to a region section
                self.previous_event = None;
                // and reset our region to None as well
                self.current_region = None;
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                linter_state: self.linter_state.to_string(),
                line_type: line_type.to_string(),
                expected_line_types: vec![
                    EVENT_DATE_LOCATION_GROUP_TYPE.to_string(),
                    NEWLINE_TYPE.to_string(),
                ],
            }),
        }
    }

    fn handle_expecting_event_name_link(
        &mut self,
        line_type: EventLineType,
    ) -> Result<(), LintError> {
        match line_type {
            EventLineType::EventName => {
                self.linter_state = self.linter_state.next()?;
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                linter_state: self.linter_state.to_string(),
                line_type: line_type.to_string(),
                expected_line_types: vec![EVENT_NAME_TYPE.to_string()],
            }),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn build_event_section(body_to_add: Option<&str>) -> String {
        let mut text = "some pre events section text\n".to_owned();
        text.push_str("## Upcoming Events\n\n");
        // just pushing each line separately to make it a little neater looking here, rather than one huge string literal
        text.push_str("Rusty Events between 2024-10-23 - 2024-11-20 🦀\n\n");
        text.push_str("### Virtual\n");
        text.push_str(
            "* 2024-10-24 | Virtual | [Women in Rust](https://www.meetup.com/women-in-rust/)\n",
        );
        text.push_str("    * [**Part 4 of 4 - Hackathon Showcase: Final Projects and Presentations**](https://www.meetup.com/women-in-rust/events/303213835/)\n");
        text.push('\n');

        if let Some(lines) = body_to_add {
            text.push_str(lines);
        }

        text.push_str("If you are running a Rust event please add it to the [calendar] to get\n");
        text.push_str("it mentioned here. Please remember to add a link to the event too.\n");

        text
    }

    #[test]
    fn test_valid_event_section() -> TestResult {
        let mut linter = EventSectionLinter::default();
        let text = build_event_section(None);
        Ok(linter.lint(&text)?)
    }
}
