use std::fmt;

use chrono::NaiveDate;
use log::{debug, error};

use crate::{
    constants::*,
    line_types::{EventDateLocationGroup, EventLineType},
    reader::{Line, LineError, Reader},
};

// TODO:
// - lint for empty regions
// - clean up errors and error messages
// - tests
// - add tools for adding new events
// - check for duplicated links
// - make sure each location in virtual section starts with "virtual"

/// Linter errors
#[derive(Debug, PartialEq, Eq)]
pub enum LintError<'a> {
    UnexpectedDateRange {
        line: Line<'a>,
    },
    UnexpectedLineType {
        line: Line<'a>,
        linter_state: LinterState,
        expected_line_types: Vec<&'static str>,
    },
    EventOutOfDateRange {
        line: Line<'a>,
        event_date: NaiveDate,
        date_range: (NaiveDate, NaiveDate),
    },
    EventOutOfOrder {
        line: Line<'a>,
    },
    DateRangeNotSet {
        line: Line<'a>,
    },
    UnexpectedEnd,
    // TODO: add error message here?
    LintFailed,
    LineParseFailed(LineError),
    ExpectedRegionHeader {
        line: Line<'a>,
    },
}

impl<'a> From<LineError> for LintError<'a> {
    fn from(value: LineError) -> Self {
        Self::LineParseFailed(value)
    }
}

impl<'a> fmt::Display for LintError<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let error_msg = match self {
            Self::UnexpectedDateRange { line } => {
                format!("multiple date ranges found\n{}", line)
            }
            Self::UnexpectedLineType {
                line,
                linter_state,
                expected_line_types,
            } => {
                let expected_types = expected_line_types.join(" ");
                let expected_types = expected_types.trim();
                format!(
                    "linter in state '{}', expected line type(s): '{}', found\n{}",
                    linter_state, expected_types, line
                )
            }
            Self::EventOutOfDateRange {
                line,
                event_date,
                date_range,
            } => {
                format!(
                    "event date '{}' does not fall within newsletter date range '{} - {}'\n{}",
                    event_date, date_range.0, date_range.1, line
                )
            }
            Self::EventOutOfOrder { line } => {
                format!(
                    "event should be after previous event date, not before\n{}",
                    line
                )
            }
            Self::DateRangeNotSet { line } => {
                format!(
                    "found an event date but we haven't set the date range to compare it to\n{}",
                    line
                )
            }
            Self::UnexpectedEnd => "reached unexpected end of file".to_owned(),
            Self::LintFailed => "lint failed! see above for error details".to_owned(),
            Self::LineParseFailed(twir_line_error) => twir_line_error.to_string(),
            Self::ExpectedRegionHeader { line } => {
                format!("header did not match an expected region\n{}", line)
            }
        };

        write!(f, "{}", error_msg)
    }
}

impl<'a> std::error::Error for LintError<'a> {}

/// Overall state of the linter, keeps track of what "section" we are in
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinterState {
    /// Not yet in event section
    PreEvents,
    /// Expecting our event date range, which we will use to verify event fall within our expected range
    ExpectingDateRange,
    /// Expecting a regional event section (e.g. Virtual, Asia, Europe, etc)
    ExpectingRegion,
    /// Expecting a date, location, and group event line
    ExpectingEventDate,
    /// Expecting an event name and event link
    ExpectingEventInfo,
    /// We have finished reading the entire event section
    Done,
}

impl LinterState {
    fn new() -> Self {
        Self::PreEvents
    }
}

impl fmt::Display for LinterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::PreEvents => "PreEvents",
            Self::ExpectingDateRange => "ExpectingDateRange",
            Self::ExpectingRegion => "ExpectingRegion",
            Self::ExpectingEventDate => "ExpectingEventDate",
            Self::ExpectingEventInfo => "ExpectingEventInfo",
            Self::Done => "Done",
        };
        write!(f, "{}", s)
    }
}

// TODO: keep track of newlines here, like in a counter? So we can lint for unexpected newlines between sections
#[derive(Debug)]
pub struct EventLinter {
    /// Our current state of the linter
    linter_state: LinterState,
    /// Date range - this is unknown until we reach the date range line. Used for validating dates fall within the given range
    event_date_range: Option<(NaiveDate, NaiveDate)>,
    /// Region we are in
    current_region: Option<String>,
    /// The last event in our current region. Used to make sure we have our events properly sorted by date and location name
    previous_event: Option<EventDateLocationGroup>,
    /// Maximum error count before bailing
    error_limit: u32,
}

impl EventLinter {
    pub fn new(error_limit: u32) -> Self {
        Self {
            linter_state: LinterState::new(),
            event_date_range: None,
            current_region: None,
            previous_event: None,
            error_limit,
        }
    }

    pub fn lint(&mut self, reader: Reader) -> Result<(), LintError> {
        let mut error_count: u32 = 0;

        for line in reader {
            let line = line?;
            match self.lint_line(&line) {
                Ok(_) => (),
                Err(e) => {
                    error!("{}", e);

                    // attempt to continue to parse, this could print out a bunch of errors in some cases
                    // setting the next state is a total guess here and only makes sense in a few states
                    self.linter_state = match self.linter_state {
                        LinterState::ExpectingEventDate => LinterState::ExpectingEventInfo,
                        LinterState::ExpectingEventInfo => LinterState::ExpectingEventDate,
                        _ => return Err(LintError::LintFailed),
                    };

                    error_count += 1;

                    // if we reach this many errors something has probably gone very wrong, so just exit early
                    // rather than overwhelming the output with more error messages
                    if error_count == self.error_limit {
                        error!("reached our maximum error limit, bailing");
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

    fn lint_line<'a>(&'a mut self, line: &'a Line<'a>) -> Result<(), LintError<'a>> {
        debug!(
            "in state {}, parsed {}",
            self.linter_state.to_string(),
            line.to_string(),
        );

        match &self.linter_state {
            LinterState::PreEvents => {
                self.handle_pre_events(line);
                Ok(())
            }
            LinterState::ExpectingDateRange => self.handle_expecting_date_range(line),
            LinterState::ExpectingRegion => self.handle_expecting_region(line),
            LinterState::ExpectingEventDate => self.handle_expecting_event_date(line),
            LinterState::ExpectingEventInfo => self.handle_expecting_event_info(line),
            LinterState::Done => Ok(()),
        }
    }

    /// Handler before we are in the events section. Accepts all lines and just continues until we hit the event section
    fn handle_pre_events(&mut self, line: &Line) {
        if line.get_line_type() == &EventLineType::StartEventSection {
            self.linter_state = LinterState::ExpectingDateRange;
        }
    }

    /// Handler to run when we are expecting to receive a date range line
    fn handle_expecting_date_range(&mut self, line: &Line) -> Result<(), LintError> {
        match line.get_line_type() {
            EventLineType::Newline => Ok(()),
            EventLineType::EventsDateRange(start_date, end_date) => {
                if self.event_date_range.is_none() {
                    self.event_date_range = Some((*start_date, *end_date));
                    self.linter_state = LinterState::ExpectingRegion;
                    Ok(())
                } else {
                    Err(LintError::UnexpectedDateRange {
                        line: line.clone().into_owned(),
                    })
                }
            }
            _ => Err(LintError::UnexpectedLineType {
                line: line.clone().into_owned(),
                linter_state: self.linter_state,
                expected_line_types: vec![NEWLINE_TYPE, EVENTS_DATE_RANGE_TYPE],
            }),
        }
    }

    fn handle_expecting_region<'a>(&mut self, line: &'a Line) -> Result<(), LintError<'a>> {
        match line.get_line_type() {
            EventLineType::Newline => Ok(()),
            EventLineType::Header(maybe_region) => {
                if let Some(region) = maybe_region {
                    // TODO: check if region is already set
                    self.current_region = Some(region.clone());
                    self.linter_state = LinterState::ExpectingEventDate;
                    Ok(())
                } else {
                    Err(LintError::ExpectedRegionHeader {
                        line: line.clone().to_owned(),
                    })
                }
            }
            EventLineType::EndEventSection => {
                self.linter_state = LinterState::Done;
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                line: line.clone().to_owned(),
                linter_state: self.linter_state,
                expected_line_types: vec![
                    NEWLINE_TYPE,
                    EVENT_REGION_HEADER_TYPE,
                    END_EVENTS_SECTION,
                ],
            }),
        }
    }

    fn handle_expecting_event_date<'a>(&mut self, line: &'a Line) -> Result<(), LintError<'a>> {
        match line.get_line_type() {
            EventLineType::EventDate(event_date_location) => {
                // validate event is within date range
                if let Some(date_range) = &self.event_date_range {
                    if (event_date_location.date() < date_range.0)
                        || (event_date_location.date() > date_range.1)
                    {
                        return Err(LintError::EventOutOfDateRange {
                            line: line.clone().to_owned(),
                            event_date: event_date_location.date(),
                            date_range: *date_range,
                        });
                    }
                // if we don't have the date range set, we are in an unexpected state
                } else {
                    return Err(LintError::DateRangeNotSet {
                        line: line.clone().to_owned(),
                    });
                }

                // if there is a previous event, compare to make sure our current one is later than the previous one
                if let Some(previous_event) = &self.previous_event {
                    if event_date_location < previous_event {
                        return Err(LintError::EventOutOfOrder {
                            line: line.clone().to_owned(),
                        });
                    }
                }

                // and save our previous event so we can compare it when looking at the next event
                self.previous_event = Some(event_date_location.clone());
                self.linter_state = LinterState::ExpectingEventInfo;

                Ok(())
            }
            // If we hit a newline it should mean that we are done with a given regional section (Virtual, Asia, etc)
            EventLineType::Newline => {
                self.linter_state = LinterState::ExpectingRegion;
                // and reset our previous event to None, ordering is only internal to a region section
                self.previous_event = None;
                // and reset our region to None as well
                self.current_region = None;
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                line: line.clone().to_owned(),
                linter_state: self.linter_state,
                expected_line_types: vec![EVENT_DATE_LOCATION_GROUP_TYPE, NEWLINE_TYPE],
            }),
        }
    }

    fn handle_expecting_event_info<'a>(&mut self, line: &'a Line) -> Result<(), LintError<'a>> {
        match line.get_line_type() {
            EventLineType::EventInfo(_event_name_urls) => {
                self.linter_state = LinterState::ExpectingEventDate;
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                line: line.clone().to_owned(),
                linter_state: self.linter_state,
                expected_line_types: vec![EVENT_NAME_TYPE],
            }),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn build_event_section(body_to_add: Option<&str>) -> String {
        let mut text = "some pre events section text\n".to_owned();
        text.push_str("## Upcoming Events\n\n");
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
    fn test_valid_event_section() {
        let text = build_event_section(None);
        let reader = TwirReader::new(&text);
        let mut linter = EventLinter::default();
        linter.lint(reader).unwrap();
    }
}
