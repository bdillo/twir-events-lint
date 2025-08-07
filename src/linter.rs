use std::fmt;

use chrono::NaiveDate;
use log::{debug, error};

use crate::reader::{EventDate, EventOverview, Line, ParsedLine, Reader};

// TODO:
// - lint for empty regions
// - check for duplicated links
// - check meetup urls don't have that tracker in them

/// Linter errors
#[derive(Debug, PartialEq, Eq)]
pub enum LintError {
    // TODO: re-add expected types here somehow
    DateRangeNotSet,
    UnexpectedLineType {
        line: Line<'static>,
        linter_state: LinterState,
    },
    EventOutOfDateRange {
        line: Line<'static>,
        event_date: EventDate,
        start: NaiveDate,
        end: NaiveDate,
    },
    EventOutOfOrder {
        line: Line<'static>,
    },
    // TODO: add error message here?
    LintFailed,
    ExpectedRegionHeader {
        line: Line<'static>,
    },
}

impl fmt::Display for LintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let error_msg = match self {
            LintError::UnexpectedLineType { line, linter_state } => {
                format!("linter in state '{}', found:\n{}", linter_state, line)
            }
            LintError::EventOutOfDateRange {
                line,
                event_date,
                start,
                end,
            } => {
                format!(
                    "event date '{}' does not fall within newsletter date range '{} - {}'\n{}",
                    event_date, start, end, line
                )
            }
            LintError::EventOutOfOrder { line } => {
                format!(
                    "event should be after previous event date, not before\n{}",
                    line
                )
            }
            LintError::LintFailed => "lint failed, see above for error details".to_owned(),
            LintError::ExpectedRegionHeader { line } => todo!(),
            LintError::DateRangeNotSet => todo!(),
        };

        write!(f, "{}", error_msg)
    }
}

impl std::error::Error for LintError {}

/// Overall state of the linter, keeps track of what section we are in
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinterState {
    /// Expecting the start of the event section
    ExpectingStartEventSection,
    /// Expecting the date range for the newletter's events
    ExpectingEventsDateRange,
    /// Expecting a regional event section (e.g. Virtual, Asia, Europe, etc)
    ExpectingRegionHeader,
    /// Expecting a date, location, and group event line
    ExpectingEventOverview,
    /// Expecting an event name and event link
    ExpectingEventLinks,
    /// We have finished reading the entire event section
    Done,
}

impl LinterState {
    fn new() -> Self {
        Self::ExpectingStartEventSection
    }
}

impl fmt::Display for LinterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LinterState::ExpectingStartEventSection => "ExpectingStartEventSection",
            LinterState::ExpectingEventsDateRange => "ExpectingEventsDateRange",
            LinterState::ExpectingRegionHeader => "ExpectingRegion",
            LinterState::ExpectingEventOverview => "ExpectingEventOverview",
            LinterState::ExpectingEventLinks => "ExpectingEventLinks",
            LinterState::Done => "Done",
        };
        write!(f, "{}", s)
    }
}

/// The state machine for linting the events section
// TODO: keep track of newlines here, like in a counter? So we can lint for unexpected newlines between sections
// TODO: move the reader back into the linter i think
#[derive(Debug)]
pub struct EventLinter {
    /// Current state of the linter
    state: LinterState,
    /// Start date for newsletter
    start: Option<NaiveDate>,
    /// End date for newsletter
    end: Option<NaiveDate>,
    /// Region we are currently reading
    current_region: Option<String>,
    /// The last event's date and location in our current region. Used to make sure we have our events properly sorted
    previous_overview: Option<EventOverview>,
    /// Current error count
    error_count: u16,
    /// Maximum error count before bailing
    error_limit: u16,
}

impl EventLinter {
    pub fn new(error_limit: u16) -> Self {
        Self {
            state: LinterState::new(),
            start: None,
            end: None,
            current_region: None,
            previous_overview: None,
            error_count: 0,
            error_limit,
        }
    }

    pub fn lint(&mut self, mut reader: Reader) -> Result<(), LintError> {
        while let Some(line) = reader.next() {
            // TODO: fix
            let line = line.unwrap();
            self.lint_line(&line)?;
        }
        todo!()
    }

    fn lint_line(&mut self, line: &Line) -> Result<(), LintError> {
        debug!(
            "in state {}, linting line #{}",
            self.state.to_string(),
            line.num(),
        );

        let lint_result = match &self.state {
            LinterState::ExpectingStartEventSection => todo!(),
            LinterState::ExpectingEventsDateRange => todo!(),
            LinterState::ExpectingRegionHeader => self.expecting_region(line),
            LinterState::ExpectingEventOverview => self.expecting_event_overview(line),
            LinterState::ExpectingEventLinks => self.expecting_event_links(line),
            LinterState::Done => Ok(()),
        };

        match lint_result {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{}", e);

                // attempt to continue to parse, this could print out a bunch of errors in some cases
                // setting the next state is a total guess here and only makes sense in a few states
                self.state = match self.state {
                    LinterState::ExpectingEventOverview => LinterState::ExpectingEventLinks,
                    LinterState::ExpectingEventLinks => LinterState::ExpectingEventOverview,
                    _ => return Err(LintError::LintFailed),
                };

                self.error_count += 1;

                // if we reach this many errors something has probably gone very wrong, so just exit early
                // rather than overwhelming the output with more error messages
                if self.error_count == self.error_limit {
                    error!("reached our maximum error limit, bailing");
                    Err(LintError::LintFailed)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Helper to see if a given date falls within the newsletter's range
    fn date_in_scope(&self, date: &NaiveDate) -> Result<bool, LintError> {
        if let Some(start) = self.start
            && let Some(end) = self.end
        {
            Ok(date >= &start || date <= &end)
        } else {
            Err(LintError::DateRangeNotSet)
        }
    }

    /// Expecting a region header, newlines are ok here, as well as the end of the events section
    fn expecting_region(&mut self, line: &Line) -> Result<(), LintError> {
        match line.parsed() {
            ParsedLine::Newline => Ok(()),
            ParsedLine::RegionHeader(region) => {
                // TODO: check if region is already set
                self.current_region = Some(region.clone());
                self.state = LinterState::ExpectingEventOverview;
                Ok(())
            }
            ParsedLine::EndEventSection => {
                self.state = LinterState::Done;
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                line: line.clone().to_owned(),
                linter_state: self.state,
            }),
        }
    }

    fn expecting_event_overview(&mut self, line: &Line) -> Result<(), LintError> {
        match line.parsed() {
            ParsedLine::EventOverview(overview) => {
                // validate event is within date range
                match overview.date() {
                    // if it's just a single date, make sure its within the newsletter's range
                    EventDate::Date(event_date) => {
                        if !self.date_in_scope(event_date)? {
                            return Err(LintError::EventOutOfDateRange {
                                line: line.to_owned(),
                                event_date: *overview.date(),
                                // TODO: cleanup
                                start: self.start.unwrap(),
                                end: self.end.unwrap(),
                            });
                        }
                    }
                    // if the event is a date range, see if either the start OR the end dates fall witin our range
                    EventDate::DateRange { start, end } => {
                        let start_in_scope = self.date_in_scope(start)?;
                        let end_in_scope = self.date_in_scope(end)?;

                        if !(start_in_scope && end_in_scope) {
                            return Err(LintError::EventOutOfDateRange {
                                line: line.to_owned(),
                                event_date: *overview.date(),
                                // TODO: cleanup
                                start: self.start.unwrap(),
                                end: self.end.unwrap(),
                            });
                        }
                    }
                }

                // if there is a previous event, compare to make sure our current one is later than the previous one
                if let Some(prev_overview) = &self.previous_overview {
                    if overview < prev_overview {
                        return Err(LintError::EventOutOfOrder {
                            line: line.to_owned(),
                        });
                    }
                }

                // and save our previous event so we can compare it when looking at the next event
                self.previous_overview = Some(overview.clone());
                self.state = LinterState::ExpectingEventLinks;

                Ok(())
            }
            // If we hit a newline it should mean that we are done with a given regional section (Virtual, Asia, etc)
            ParsedLine::Newline => {
                self.state = LinterState::ExpectingRegionHeader;
                // and reset our previous event to None, ordering is only internal to a region section
                self.previous_overview = None;
                // and reset our region to None as well
                self.current_region = None;
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                line: line.to_owned(),
                linter_state: self.state,
            }),
        }
    }

    fn expecting_event_links(&mut self, line: &Line) -> Result<(), LintError> {
        match line.parsed() {
            ParsedLine::EventLinks(_links) => {
                self.state = LinterState::ExpectingEventOverview;
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                line: line.to_owned(),
                linter_state: self.state,
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
        let reader = Reader::new(&text);
        let mut linter = EventLinter::new(20);
        linter.lint(reader).unwrap();
    }
}
