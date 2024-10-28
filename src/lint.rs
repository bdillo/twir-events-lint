use std::{error::Error, fmt::Display, str::FromStr, sync::LazyLock};

use chrono::{NaiveDate, ParseError};
use log::debug;
use regex::Regex;

const START_EVENTS_SECTION: &str = "## Upcoming Events";
const EVENTS_DATE_RANGE: &str = "Rusty Events between";
const EVENT_REGION_HEADER: &str = "### ";
// TODO: fix this
const EVENT_DATE_LOCATION_GROUP: &str = "* 2024";
const EVENT_NAME_LINK: &str = "    * [**";
const END_EVENTS_SECTION: &str = "If you are running a Rust event please add it to the [calendar]";

const DATE_RE_STR: &str = r"\d{4}-\d{1,2}-\d{1,2}";

static EVENT_DATE_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    let re_str = format!(r".*({})\s+-\s+({})", DATE_RE_STR, DATE_RE_STR);
    Regex::new(&re_str).expect("Failed to compile regex!")
});
static EVENT_DATE_LOCATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    let re_str = format!(r"\* ({}) \| (.*) \| ", DATE_RE_STR);
    Regex::new(&re_str).expect("Failed to compile regex!")
});

/// An error linting - this error should provide enough information by itself to be useful to a user
#[derive(Debug)]
pub enum LintError {
    InvalidStateChange {
        from: LinterState,
    },
    UnexpectedDateRange {
        line_num: usize,
    },
    UnexpectedLineType {
        line_num: usize,
        linter_state: LinterState,
        line_type: EventLineType,
    },
    EventOutOfDateRange {
        line_num: usize,
        line: String,
        event_date: NaiveDate,
        date_range: (NaiveDate, NaiveDate),
    },
    EventOutOfOrder {
        line_num: usize,
        line: String,
        event: EventDateLocation,
        previous_event: EventDateLocation,
    },
    DateRangeNotSet {
        line_num: usize,
        line: String,
    },
    RegexError {
        regex_string: String,
        line: String,
    },
    DateParseError {
        line: String,
        chrono_error: ParseError,
    },
    // TODO: generic error - clean this up later
    ParseError {
        line: String,
    },
}

impl Display for LintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: do this
        write!(f, "error!")
    }
}

impl Error for LintError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
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
    EventsDateRange {
        start_date: NaiveDate,
        end_date: NaiveDate,
    },
    /// Header of a new regional section, "### Virtual", "### Asia"...
    EventRegionHeader { region: String },
    /// First line of an event with the date, location, and group link "* 2024-10-24 | Virtual | [Women in Rust]..."
    EventDateLocationGroupLink(EventDateLocation),
    /// Event name and link to specific event " * [**Part 4 of 4 - Hackathon Showcase: Final Projects and Presentations**]..."
    EventNameLink,
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
        // TODO: just use regex here
        let parsed = match s {
            _ if s.is_empty() => Self::Newline,
            _ if s.starts_with(START_EVENTS_SECTION) => Self::StartEventSection,
            s if s.starts_with(EVENTS_DATE_RANGE) => {
                let parsed_time_range = Self::extract_date_range(s)?;
                Self::EventsDateRange {
                    start_date: parsed_time_range.0,
                    end_date: parsed_time_range.1,
                }
            }
            s if s.starts_with(EVENT_REGION_HEADER) => {
                let region = Self::extract_region_header(s)?;
                Self::EventRegionHeader {
                    region: region.to_owned(),
                }
            }
            s if s.starts_with(EVENT_DATE_LOCATION_GROUP) => {
                let (date, location) = Self::extract_date_location_group(s)?;
                Self::EventDateLocationGroupLink(EventDateLocation {
                    date,
                    location: location.to_owned(),
                })
            }
            _ if s.starts_with(EVENT_NAME_LINK) => Self::EventNameLink,
            _ if s.starts_with(END_EVENTS_SECTION) => Self::EndEventSection,
            _ => Self::Unrecognized,
        };

        Ok(parsed)
    }
}

impl EventLineType {
    fn extract_date_range(line: &str) -> Result<(NaiveDate, NaiveDate), LintError> {
        let re = &*EVENT_DATE_RANGE_RE;
        // TODO: clean up repetition
        let captures = re.captures(line).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
            line: line.to_owned(),
        })?;

        debug!("Captured: '{:?}'", &captures);

        let start_capture = captures.get(1).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
            line: line.to_owned(),
        })?;

        let end_capture = captures.get(2).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
            line: line.to_owned(),
        })?;

        let start_parsed =
            start_capture
                .as_str()
                .parse::<NaiveDate>()
                .map_err(|e| LintError::DateParseError {
                    line: line.to_owned(),
                    chrono_error: e,
                })?;

        let end_parsed =
            end_capture
                .as_str()
                .parse::<NaiveDate>()
                .map_err(|e| LintError::DateParseError {
                    line: line.to_owned(),
                    chrono_error: e,
                })?;

        Ok((start_parsed, end_parsed))
    }

    fn extract_region_header(line: &str) -> Result<&str, LintError> {
        let region =
            line.strip_prefix(EVENT_REGION_HEADER)
                .ok_or_else(|| LintError::ParseError {
                    line: line.to_owned(),
                })?;

        Ok(region)
    }

    fn extract_date_location_group(line: &str) -> Result<(NaiveDate, &str), LintError> {
        let re = &*EVENT_DATE_LOCATION_RE;
        // TODO: clean up this repetition
        let captures = re.captures(line).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
            line: line.to_owned(),
        })?;

        debug!("Captured: '{:?}'", &captures);

        let date_capture = captures.get(1).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
            line: line.to_owned(),
        })?;

        let region_capture = captures.get(2).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
            line: line.to_owned(),
        })?;

        let date_parsed =
            date_capture
                .as_str()
                .parse::<NaiveDate>()
                .map_err(|e| LintError::DateParseError {
                    line: line.to_owned(),
                    chrono_error: e,
                })?;

        let region = region_capture.as_str();

        Ok((date_parsed, region))
    }
}

/// Overall state of the linter, keeps track of what "section" we are in
#[derive(Clone, Debug)]
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
            _ => Err(LintError::InvalidStateChange { from: self.clone() }),
        }
    }

    fn finish_regional_section(&self) -> Result<Self, LintError> {
        match self {
            Self::ExpectingEventDateLocationGroupLink => Ok(Self::ExpectingRegionalHeader),
            _ => Err(LintError::InvalidStateChange { from: self.clone() }),
        }
    }

    fn finish(&self) -> Result<Self, LintError> {
        todo!()
    }
}

#[derive(Debug)]
pub struct EventSectionLinter {
    linter_state: LinterState,
    // TODO: make another date range struct? or is this fine. In EventLineType it's used in struct format in the enum variant, should be consistent
    event_date_range: Option<(NaiveDate, NaiveDate)>,
    current_region: Option<String>,
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

        for (i, line) in lines.iter().enumerate() {
            self.read_line(i, line)?;
        }

        // TODO: verify we are in Done state

        Ok(())
    }

    fn read_line(&mut self, line_num: usize, line: &str) -> Result<(), LintError> {
        match &self.linter_state {
            LinterState::PreEvents => self.handle_pre_events(line_num, line),
            LinterState::ExpectingDateRange => self.handle_expected_date_range(line_num, line),
            LinterState::ExpectingRegionalHeader => {
                self.handle_expecting_regional_header(line_num, line)
            }
            LinterState::ExpectingEventDateLocationGroupLink => {
                self.handle_expecting_event_date_location_group_link(line_num, line)
            }
            LinterState::ExpectingEventNameLink => {
                self.handle_expecting_event_name_link(line_num, line)
            }
            _ => panic!("linter state panic!"),
        }
    }

    /// Handler before we are in the events section. Accepts all lines and just continues until we hit the event section
    fn handle_pre_events(&mut self, line_num: usize, line: &str) -> Result<(), LintError> {
        // TODO: fix this line type logic, maybe a hint for how to parse the line?
        let line_type = line.parse::<EventLineType>()?;
        debug!("Parsed line #{} '{}' as '{:?}'", line_num, line, line_type);

        match line_type {
            EventLineType::StartEventSection => {
                self.linter_state = self.linter_state.next()?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Handler to run when we are expecting to receive a date range line
    fn handle_expected_date_range(&mut self, line_num: usize, line: &str) -> Result<(), LintError> {
        let line_type = line.parse::<EventLineType>()?;
        debug!("Parsed line #{} '{}' as '{:?}'", line_num, line, line_type);

        match line_type {
            EventLineType::Newline => Ok(()),
            EventLineType::EventsDateRange {
                start_date,
                end_date,
            } => {
                if self.event_date_range.is_none() {
                    self.event_date_range = Some((start_date, end_date));
                    self.linter_state = self.linter_state.next()?;
                    Ok(())
                } else {
                    Err(LintError::UnexpectedDateRange { line_num })
                }
            }
            _ => Err(LintError::UnexpectedLineType {
                line_num,
                linter_state: self.linter_state.clone(),
                line_type,
            }),
        }
    }

    fn handle_expecting_regional_header(
        &mut self,
        line_num: usize,
        line: &str,
    ) -> Result<(), LintError> {
        let line_type = line.parse::<EventLineType>()?;
        debug!("Parsed line #{} '{}' as '{:?}'", line_num, line, line_type);

        match line_type {
            EventLineType::Newline => Ok(()),
            EventLineType::EventRegionHeader { region } => {
                // TODO: check if region is already set?
                self.current_region = Some(region);
                self.linter_state = self.linter_state.next()?;
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                line_num,
                linter_state: self.linter_state.clone(),
                line_type,
            }),
        }
    }

    fn handle_expecting_event_date_location_group_link(
        &mut self,
        line_num: usize,
        line: &str,
    ) -> Result<(), LintError> {
        let line_type = line.parse::<EventLineType>()?;
        debug!("Parsed line #{} '{}' as '{:?}'", line_num, line, line_type);

        match line_type {
            EventLineType::EventDateLocationGroupLink(event_date_location) => {
                // validate event is within date range
                if let Some(date_range) = &self.event_date_range {
                    if (event_date_location.date < date_range.0)
                        || (event_date_location.date > date_range.1)
                    {
                        return Err(LintError::EventOutOfDateRange {
                            line_num,
                            line: line.to_owned(),
                            event_date: event_date_location.date,
                            date_range: *date_range,
                        });
                    }
                // if we don't have the date range set, we are in an unexpected state
                } else {
                    return Err(LintError::DateRangeNotSet {
                        line_num,
                        line: line.to_owned(),
                    });
                }

                // if there is a previous event, compare to make sure our current one is later than the previous one
                if let Some(previous_event) = &self.previous_event {
                    // TODO: make sure this comparison is correct
                    if event_date_location > *previous_event {
                        return Err(LintError::EventOutOfOrder {
                            line_num,
                            line: line.to_string(),
                            event: event_date_location,
                            previous_event: previous_event.clone(),
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
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                line_num,
                linter_state: self.linter_state.clone(),
                line_type,
            }),
        }
    }

    fn handle_expecting_event_name_link(
        &mut self,
        line_num: usize,
        line: &str,
    ) -> Result<(), LintError> {
        let line_type = line.parse::<EventLineType>()?;
        debug!("Parsed line #{} '{}' as '{:?}'", line_num, line, line_type);

        match line_type {
            EventLineType::EventNameLink => {
                self.linter_state = self.linter_state.next()?;
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                line_num,
                linter_state: self.linter_state.clone(),
                line_type,
            }),
        }
    }
}
