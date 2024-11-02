use core::fmt;
use std::{error::Error, fmt::Display, str::FromStr, sync::LazyLock};

use chrono::{NaiveDate, ParseError};
use log::{debug, error};
use regex::Regex;

/// Lines we expect to match exactly
const START_EVENTS_SECTION: &str = "## Upcoming Events";
const EVENT_REGION_HEADER: &str = "### ";
const END_EVENTS_SECTION: &str = "If you are running a Rust event please add it to the [calendar]";

/// Hints for what type of line we are parsing - this helps us generate a bit better error messages
const EVENTS_DATE_RANGE_HINT: &str = "Rusty Events between";
const EVENT_DATE_LOCATION_GROUP_HINT: &str = "* ";
const EVENT_NAME_HINT: &str = "    * ";

/// Regex for grabbing timestamps - we use chrono to parse this and do the actual validation
const DATE_RE_STR: &str = r"\d{4}-\d{1,2}-\d{1,2}";

/// Line "types" in the event section. We use this in several different stringy contexts, so just hardcode the strings here
/// See EventLineType for a description of each type
const NEWLINE_TYPE: &str = "Newline";
const START_EVENT_SECTION_TYPE: &str = "StartEventSection";
const EVENTS_DATE_RANGE_TYPE: &str = "EventsDateRange";
const EVENT_REGION_HEADER_TYPE: &str = "EventRegionHeader";
const EVENT_DATE_LOCATION_GROUP_TYPE: &str = "EventDateLocationGroup";
const EVENT_NAME_TYPE: &str = "EventName";
const END_EVENT_SECTION_TYPE: &str = "EndEventSection";
const UNRECOGNIZED_TYPE: &str = "Unrecognized";

static EVENT_DATE_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    let re_str = format!(
        r"{} ({}) - ({})",
        EVENTS_DATE_RANGE_HINT, DATE_RE_STR, DATE_RE_STR
    );
    Regex::new(&re_str).expect("Failed to compile regex!")
});

static EVENT_DATE_LOCATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    let re_str = format!(r"\* ({}) \| (.*) \| (.*)", DATE_RE_STR);
    Regex::new(&re_str).expect("Failed to compile regex!")
});

static EVENT_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"    \* \[\*\*.*\*\*\]\(.*\)").expect("Failed to compile regex!"));

// TODO:
// - lint actual line contents (like everything is formatted strictly)
// - check for tracker things in urls
// - lint for empty regions
// - clean up errors and error messages
// - tests
// - add tools for adding new events
// - validate urls
// - check for duplicated links

/// An error linting - this error should provide enough information by itself to be useful to a user
#[derive(Debug)]
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
    InvalidUrl,
}

impl Display for LintError {
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
            Self::ParseError => "Parse error".to_owned(), // TODO: is this needed?
            Self::UnexpectedEnd => "Reached unexpected end of file".to_owned(),
            Self::LintFailed => "Lint failed! See above for error details".to_owned(),
            _ => todo!(),
        };

        write!(f, "{}", error_msg)
    }
}

/// An event's date and location. Used to ensure our dates are ordered correctly, first by date, then by location
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EventDateLocation {
    date: NaiveDate,
    location: String,
}

/// The type of a given line of text in the event section
#[derive(Debug)]
enum EventLineType {
    /// A newline
    Newline,
    /// Start of the events section, "## Upcoming Events"
    StartEventSection,
    /// The date range in the events section, "Rusty Events between..."
    EventsDateRange(NaiveDate, NaiveDate),
    /// Header of a new regional section, "### Virtual", "### Asia"...
    EventRegionHeader(String),
    /// First line of an event with the date, location, and group link "* 2024-10-24 | Virtual | [Women in Rust]..."
    EventDateLocationGroup(EventDateLocation),
    /// Event name and link to specific event " * [**Part 4 of 4 - Hackathon Showcase: Final Projects and Presentations**]..."
    EventName,
    /// End of the event section "If you are running a Rust event please add..."
    EndEventSection,
    /// A line we don't recognize - should only be lines that are not within the event section
    Unrecognized,
}

impl FromStr for EventLineType {
    // TODO: probably model this a bit differently. We should infer from the state of the linter what we expect the next line to be, rather than
    // just parsing each line without this context
    type Err = LintError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // TODO: add validation
        let parsed = match s {
            _ if s.is_empty() => Self::Newline,
            _ if s.starts_with(START_EVENTS_SECTION) => Self::StartEventSection,
            s if s.starts_with(EVENTS_DATE_RANGE_HINT) => {
                let parsed_time_range = Self::extract_date_range(s)?;
                Self::EventsDateRange(parsed_time_range.0, parsed_time_range.1)
            }
            s if s.starts_with(EVENT_REGION_HEADER) => {
                // TODO: validate regions against list of known regions
                let region = Self::extract_region_header(s)?;
                Self::EventRegionHeader(region.to_owned())
            }
            s if s.starts_with(EVENT_DATE_LOCATION_GROUP_HINT) => {
                // TODO: move this hint to be regex?
                // TODO: validate
                let (date, location) = Self::extract_date_location_group(s)?;
                Self::EventDateLocationGroup(EventDateLocation {
                    date,
                    location: location.to_owned(),
                })
            }
            s if s.starts_with(EVENT_NAME_HINT) => Self::EventName, // TODO: validate
            _ if s.starts_with(END_EVENTS_SECTION) => Self::EndEventSection,
            s => Self::Unrecognized,
        };

        Ok(parsed)
    }
}

impl fmt::Display for EventLineType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Newline => NEWLINE_TYPE,
            Self::StartEventSection => START_EVENT_SECTION_TYPE,
            Self::EventsDateRange(start, end) => {
                &format!("{}({}, {})", EVENTS_DATE_RANGE_TYPE, start, end)
            }
            Self::EventRegionHeader(region) => &format!("{}({})", EVENT_REGION_HEADER_TYPE, region),
            Self::EventDateLocationGroup(_event_date_location) => EVENT_DATE_LOCATION_GROUP_TYPE, // TODO: fix this
            Self::EventName => EVENT_NAME_TYPE,
            Self::EndEventSection => END_EVENT_SECTION_TYPE,
            Self::Unrecognized => UNRECOGNIZED_TYPE,
        };
        write!(f, "{}", s)
    }
}

impl EventLineType {
    fn extract_date_range(line: &str) -> Result<(NaiveDate, NaiveDate), LintError> {
        let re = &*EVENT_DATE_RANGE_RE;
        let captures = re.captures(line).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
        })?;

        debug!("Captured: '{:?}'", &captures);

        let start_capture = captures.get(1).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
        })?;

        let end_capture = captures.get(2).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
        })?;

        let start_parsed = start_capture
            .as_str()
            .parse::<NaiveDate>()
            .map_err(|e| LintError::DateParseError { chrono_error: e })?;

        let end_parsed = end_capture
            .as_str()
            .parse::<NaiveDate>()
            .map_err(|e| LintError::DateParseError { chrono_error: e })?;

        Ok((start_parsed, end_parsed))
    }

    fn extract_region_header(line: &str) -> Result<&str, LintError> {
        let region = line
            .strip_prefix(EVENT_REGION_HEADER)
            .ok_or(LintError::ParseError)?;

        Ok(region)
    }

    fn extract_date_location_group(line: &str) -> Result<(NaiveDate, &str), LintError> {
        let re = &*EVENT_DATE_LOCATION_RE;
        let captures = re.captures(line).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
        })?;

        debug!("Captured: '{:?}'", &captures);

        // get our required data, the date and location
        let date_capture = captures.get(1).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
        })?;

        let location_capture = captures.get(2).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
        })?;

        let date_parsed = date_capture
            .as_str()
            .parse::<NaiveDate>()
            .map_err(|e| LintError::DateParseError { chrono_error: e })?;

        let location = location_capture.as_str();

        // now we will validate the rest of the line with the group names + links. We may have more than one here as well

        // let links_capture = captures.get(3).ok_or_else(|| LintError::RegexError {
        //     regex_string: re.as_str().to_owned(),
        // })?;

        // let links_capture = links_capture.as_str();

        Ok((date_parsed, location))
    }

    fn validate_url(url: &str) -> Result<(), LintError> {
        todo!()
    }
}

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
            linter_state: LinterState::PreEvents,
            event_date_range: None,
            current_region: None,
            previous_event: None,
        }
    }
}

impl EventSectionLinter {
    pub fn lint(&mut self, md: &str) -> Result<(), LintError> {
        let lines: Vec<&str> = md.lines().collect();
        let mut has_error = false;
        let mut error_count = 0;

        for (i, line) in lines.iter().enumerate() {
            match self.read_line(i, line) {
                Ok(_) => (),
                Err(e) => {
                    // we don't care about any errors before the event section, which we expect a lot of because it's
                    // not modeled in our linter
                    if self.linter_state != LinterState::PreEvents {
                        error!("Linter Error:\n{}\nCaused by line #{}: '{}'", e, i, line);

                        if !has_error {
                            has_error = true;
                        }

                        // attempt to continue to parse, this could print out a bunch of errors in some cases
                        self.linter_state = self.linter_state.next()?;

                        error_count += 1;

                        // if we reach this many errors something has probably gone very wrong, so just exit early
                        // rather than overwhelming the output with more error messages
                        if error_count == 10 {
                            error!("Reached our maximum error limit, bailing");
                            return Err(LintError::LintFailed);
                        }
                    }
                }
            }
        }

        if self.linter_state != LinterState::Done {
            return Err(LintError::UnexpectedEnd);
        }

        if has_error {
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
                    if (event_date_location.date < date_range.0)
                        || (event_date_location.date > date_range.1)
                    {
                        return Err(LintError::EventOutOfDateRange {
                            event_date: event_date_location.date,
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
                            event_date: event_date_location.date,
                            event_location: event_date_location.location,
                            previous_event_date: previous_event.date,
                            previous_event_location: previous_event.location.to_owned(),
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
