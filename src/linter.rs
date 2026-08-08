use std::collections::HashSet;

use chrono::NaiveDate;
use log::{debug, error};

use crate::{
    events::{EventDate, EventOverview, EventsByRegion, Region},
    reader::{Line, ParsedLine, Reader},
};

// TODO: check meetup urls don't have that tracker in them

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
    EmptyRegion {
        region: Region,
        line: Line<'static>,
    },
    DuplicateEvent {
        name: String,
        url: String,
        line: Box<Line<'static>>,
    },
    ValidationFailed,
    ErrorLimitReached {
        limit: u16,
    },
    UnexpectedEnd {
        state: LinterState,
    },
    RecoveryFailed {
        state: LinterState,
    },
    InternalInvariant(&'static str),
}

impl LintError {
    fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::ValidationFailed
                | Self::ErrorLimitReached { .. }
                | Self::UnexpectedEnd { .. }
                | Self::RecoveryFailed { .. }
                | Self::InternalInvariant(_)
                | Self::DateRangeNotSet
        )
    }
}

impl std::fmt::Display for LintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let error_msg = match self {
            LintError::UnexpectedLineType { line, linter_state } => {
                format!("linter in state '{linter_state}', found:\n{line}")
            }
            LintError::EventOutOfDateRange {
                line,
                event_date,
                start,
                end,
            } => {
                format!(
                    "event date '{event_date}' does not fall within newsletter date range '{start} - {end}'\n{line}"
                )
            }
            LintError::EventOutOfOrder { line } => {
                format!("event should be after previous event date, not before\n{line}")
            }
            LintError::EmptyRegion { region, line } => {
                format!("region '{region}' has no events\n{line}")
            }
            LintError::DuplicateEvent { name, url, line } => {
                format!("duplicate event '{name}' ({url}) in region\n{line}")
            }
            LintError::ValidationFailed => "lint failed: see above logs for lint errors".to_owned(),
            LintError::ErrorLimitReached { limit } => {
                format!("lint failed: reached maximum error limit of {limit}")
            }
            LintError::UnexpectedEnd { state } => {
                format!("lint failed: unexpected end while in state '{state}'")
            }
            LintError::RecoveryFailed { state } => {
                format!("lint failed: cannot recover from state '{state}'")
            }
            LintError::InternalInvariant(message) => {
                format!("lint failed: internal invariant violated: {message}")
            }
            LintError::DateRangeNotSet => "no newsletter date range found".to_owned(),
        };

        write!(f, "{error_msg}")
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
}

impl LinterState {
    fn new() -> Self {
        Self::ExpectingStartEventSection
    }
}

impl std::fmt::Display for LinterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LinterState::ExpectingStartEventSection => "ExpectingStartEventSection",
            LinterState::ExpectingEventsDateRange => "ExpectingEventsDateRange",
            LinterState::ExpectingRegionHeader => "ExpectingRegion",
            LinterState::ExpectingEventOverview => "ExpectingEventOverview",
            LinterState::ExpectingEventLinks => "ExpectingEventLinks",
        };
        write!(f, "{s}")
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
    current_region: Option<Region>,
    /// Previous event overview, used to validate ordering within the current region
    previous_overview: Option<EventOverview>,
    /// Current event overview waiting to be paired with its event links
    pending_overview: Option<EventOverview>,
    /// Whether the current region contains at least one complete event listing
    region_has_events: bool,
    /// Current error count
    error_count: u16,
    /// Maximum error count before bailing
    error_limit: u16,
    /// Collected `EventListing`s by region, in case we want to use them outside the linter
    events: EventsByRegion,
    /// Event (name, URL) pairs seen in the current region, for duplicate detection
    seen_events: HashSet<(String, String)>,
}

impl EventLinter {
    pub fn new(error_limit: u16) -> Self {
        Self {
            state: LinterState::new(),
            start: None,
            end: None,
            current_region: None,
            previous_overview: None,
            pending_overview: None,
            region_has_events: false,
            error_count: 0,
            error_limit,
            events: EventsByRegion::new(),
            seen_events: HashSet::new(),
        }
    }

    pub fn events(&self) -> &EventsByRegion {
        &self.events
    }

    pub fn lint(&mut self, reader: Reader) -> Result<(), LintError> {
        for line in reader {
            match line {
                Ok(line) => self.lint_line(&line)?,
                Err(e) => {
                    error!("{}", e);
                    self.error_count += 1;
                    if self.error_count >= self.error_limit {
                        return Err(LintError::ErrorLimitReached {
                            limit: self.error_limit,
                        });
                    }
                    self.recover_after_parse_error();
                }
            }
        }

        if self.state != LinterState::ExpectingRegionHeader {
            return Err(LintError::UnexpectedEnd { state: self.state });
        }

        if self.error_count > 0 {
            Err(LintError::ValidationFailed)
        } else {
            Ok(())
        }
    }

    fn lint_line(&mut self, line: &Line) -> Result<(), LintError> {
        debug!(
            "in state {}, linting line #{}",
            self.state.to_string(),
            line.num(),
        );

        let lint_result = match &self.state {
            LinterState::ExpectingStartEventSection => self.expecting_start_event_section(line),
            LinterState::ExpectingEventsDateRange => self.expecting_events_date_range(line),
            LinterState::ExpectingRegionHeader => self.expecting_region(line),
            LinterState::ExpectingEventOverview => self.expecting_event_overview(line),
            LinterState::ExpectingEventLinks => self.expecting_event_links(line),
        };

        match lint_result {
            Ok(_) => Ok(()),
            Err(e) if e.is_terminal() => Err(e),
            Err(e @ LintError::UnexpectedLineType { .. }) => {
                self.record_error(e)?;
                self.recover_from_unexpected_line(line)
            }
            Err(_) => Err(LintError::InternalInvariant(
                "non-terminal error has no recovery strategy",
            )),
        }
    }

    fn recover_after_parse_error(&mut self) {
        if self.state == LinterState::ExpectingEventLinks {
            self.pending_overview = None;
            self.state = LinterState::ExpectingEventOverview;
        }
    }

    fn recover_from_unexpected_line(&mut self, line: &Line) -> Result<(), LintError> {
        match self.state {
            LinterState::ExpectingStartEventSection | LinterState::ExpectingEventsDateRange => {
                Err(LintError::RecoveryFailed { state: self.state })
            }
            LinterState::ExpectingRegionHeader => Ok(()),
            LinterState::ExpectingEventOverview | LinterState::ExpectingEventLinks => {
                match line.parsed() {
                    ParsedLine::RegionHeader(region) => {
                        self.reset_region();
                        self.current_region = Some(*region);
                        self.state = LinterState::ExpectingEventOverview;
                        Ok(())
                    }
                    ParsedLine::Newline => {
                        self.reset_region();
                        self.state = LinterState::ExpectingRegionHeader;
                        Ok(())
                    }
                    ParsedLine::EventOverview(_) => {
                        self.pending_overview = None;
                        self.state = LinterState::ExpectingEventOverview;
                        self.expecting_event_overview(line)
                    }
                    ParsedLine::EventLinks(_) => {
                        self.pending_overview = None;
                        self.state = LinterState::ExpectingEventOverview;
                        Ok(())
                    }
                    ParsedLine::StartEventSection | ParsedLine::EventsDateRange { .. } => Ok(()),
                }
            }
        }
    }

    fn reset_region(&mut self) {
        self.previous_overview = None;
        self.pending_overview = None;
        self.region_has_events = false;
        self.current_region = None;
        self.seen_events.clear();
    }

    /// Record a validation error. Logs it, increments the count, and returns Err only if the limit is hit.
    fn record_error(&mut self, lint_error: LintError) -> Result<(), LintError> {
        error!("{}", lint_error);
        self.error_count += 1;
        if self.error_count >= self.error_limit {
            Err(LintError::ErrorLimitReached {
                limit: self.error_limit,
            })
        } else {
            Ok(())
        }
    }

    /// Returns the newsletter's date range, or an error if not set
    fn date_range(&self) -> Result<(NaiveDate, NaiveDate), LintError> {
        match (self.start, self.end) {
            (Some(s), Some(e)) => Ok((s, e)),
            _ => Err(LintError::DateRangeNotSet),
        }
    }

    /// Helper to see if a given date falls within the newsletter's range
    fn date_in_scope(&self, date: &NaiveDate) -> Result<bool, LintError> {
        let (start, end) = self.date_range()?;
        Ok(date >= &start && date <= &end)
    }

    fn expecting_start_event_section(&mut self, line: &Line) -> Result<(), LintError> {
        // TODO: cleanup
        if line.parsed() == &ParsedLine::StartEventSection {
            self.state = LinterState::ExpectingEventsDateRange
        }
        Ok(())
    }

    fn expecting_events_date_range(&mut self, line: &Line) -> Result<(), LintError> {
        match line.parsed() {
            ParsedLine::Newline => Ok(()),
            ParsedLine::EventsDateRange { start, end } => {
                self.start = Some(*start);
                self.end = Some(*end);
                self.state = LinterState::ExpectingRegionHeader;
                Ok(())
            }
            _ => Err(LintError::UnexpectedLineType {
                line: line.to_owned(),
                linter_state: self.state,
            }),
        }
    }

    /// Expecting a region header, newlines are ok here, as well as the end of the events section
    fn expecting_region(&mut self, line: &Line) -> Result<(), LintError> {
        match line.parsed() {
            ParsedLine::Newline => Ok(()),
            ParsedLine::RegionHeader(region) => {
                // TODO: check if region is already set
                self.current_region = Some(*region);
                self.state = LinterState::ExpectingEventOverview;
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
                let (range_start, range_end) = self.date_range()?;

                // validate event is within date range
                let out_of_range = match overview.date() {
                    EventDate::Date(event_date) => !self.date_in_scope(event_date)?,
                    EventDate::DateRange { start, end } => {
                        !self.date_in_scope(start)? && !self.date_in_scope(end)?
                    }
                };

                if out_of_range {
                    self.record_error(LintError::EventOutOfDateRange {
                        line: line.to_owned(),
                        event_date: *overview.date(),
                        start: range_start,
                        end: range_end,
                    })?;
                }

                // if there is a previous event, compare to make sure our current one is later than the previous one
                if let Some(prev_overview) = &self.previous_overview
                    && overview < prev_overview
                {
                    self.record_error(LintError::EventOutOfOrder {
                        line: line.to_owned(),
                    })?;
                }

                // always transition — the line parsed fine even if validation failed
                self.pending_overview = Some(overview.clone());
                self.state = LinterState::ExpectingEventLinks;

                Ok(())
            }
            // If we hit a newline it should mean that we are done with a given regional section (Virtual, Asia, etc)
            ParsedLine::Newline => {
                if !self.region_has_events
                    && let Some(region) = self.current_region
                {
                    self.record_error(LintError::EmptyRegion {
                        region,
                        line: line.to_owned(),
                    })?;
                }
                self.state = LinterState::ExpectingRegionHeader;
                self.reset_region();
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
            ParsedLine::EventLinks(events) => {
                self.state = LinterState::ExpectingEventOverview;

                for event in events.iter() {
                    let key = (event.name().to_owned(), event.url().as_str().to_owned());
                    if !self.seen_events.insert(key.clone()) {
                        self.record_error(LintError::DuplicateEvent {
                            name: key.0,
                            url: key.1,
                            line: Box::new(line.to_owned()),
                        })?;
                    }
                }

                // we have the event overview and links now, so we can return a full `EventListing` in case we want to
                // do something with it outside of the linter
                let overview = self
                    .pending_overview
                    .take()
                    .ok_or(LintError::InternalInvariant("no pending overview set"))?;
                let region = self
                    .current_region
                    .ok_or(LintError::InternalInvariant("no current region set"))?;

                self.previous_overview = Some(overview.clone());
                let listing = (overview, events.to_owned()).into();
                self.events.add(listing, region);
                self.region_has_events = true;

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
    use std::fs;
    use std::path::Path;

    use crate::reader::find_events_section;

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

    fn lint_file(path: &Path) -> Result<(), LintError> {
        let content = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let (section, line_num) = find_events_section(&content)
            .unwrap_or_else(|e| panic!("failed to find events section in {}: {e}", path.display()));
        let reader = Reader::new(section, line_num);
        let mut linter = EventLinter::new(20);
        linter.lint(reader)
    }

    fn event_count(linter: &EventLinter) -> usize {
        linter
            .events()
            .into_iter()
            .map(|(_, events)| events.len())
            .sum()
    }

    fn lint_document(content: &str) -> (Result<(), LintError>, EventLinter) {
        let (section, line_num) = find_events_section(content).unwrap();
        let reader = Reader::new(section, line_num);
        let mut linter = EventLinter::new(20);
        let result = linter.lint(reader);
        (result, linter)
    }

    fn collect_fixtures(dir: &str) -> Vec<std::path::PathBuf> {
        let path = Path::new(dir);
        if !path.exists() {
            return vec![];
        }

        fs::read_dir(path)
            .unwrap()
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    Some(path)
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn test_valid_event_section() {
        let text = build_event_section(None);
        let (event_section, line_num) =
            find_events_section(&text).expect("failed to find events section");
        let reader = Reader::new(event_section, line_num);
        let mut linter = EventLinter::new(20);
        linter.lint(reader).unwrap();

        assert_eq!(event_count(&linter), 1);
        assert_eq!(linter.previous_overview, None);
        assert_eq!(linter.pending_overview, None);
        assert!(!linter.region_has_events);
    }

    #[test]
    fn event_links_require_a_pending_overview() {
        let mut reader = Reader::new(
            "    * [**Event Without Overview**](https://example.com/event/)\n",
            0,
        );
        let line = reader.next().unwrap().unwrap();
        let mut linter = EventLinter::new(20);
        linter.state = LinterState::ExpectingEventLinks;
        linter.current_region = Some(Region::Virtual);

        assert_eq!(
            linter.lint_line(&line),
            Err(LintError::InternalInvariant("no pending overview set"))
        );
        assert!(!linter.region_has_events);
        assert!(linter.events().into_iter().next().is_none());
    }

    #[test]
    fn incomplete_document_reports_its_final_state() {
        let reader = Reader::new("## Upcoming Events\n", 0);
        let mut linter = EventLinter::new(20);

        assert_eq!(
            linter.lint(reader),
            Err(LintError::UnexpectedEnd {
                state: LinterState::ExpectingEventsDateRange,
            })
        );
    }

    #[test]
    fn stops_at_error_limit() {
        let extra = concat!(
            "### Europe\n",
            "* 2024-09-01 | Berlin, DE | [Rust Berlin](https://www.meetup.com/rust-berlin/)\n",
            "    * [**Out of Range Event**](https://www.meetup.com/rust-berlin/events/000001/)\n",
            "\n",
        );

        let text = build_event_section(Some(extra));
        let (section, line_num) = find_events_section(&text).unwrap();
        let reader = Reader::new(section, line_num);
        let mut linter = EventLinter::new(1);

        assert_eq!(
            linter.lint(reader),
            Err(LintError::ErrorLimitReached { limit: 1 })
        );
        assert_eq!(linter.error_count, 1);
    }

    #[test]
    fn out_of_range_event_does_not_cascade() {
        let _ = simple_logger::init_with_level(log::Level::Error);

        // An out-of-range event followed by valid events should produce exactly
        // one error (the out-of-range event), not a cascade of state desync errors.
        let extra = concat!(
            "### Europe\n",
            "* 2024-09-01 | Berlin, DE | [Rust Berlin](https://www.meetup.com/rust-berlin/)\n",
            "    * [**Out of Range Event**](https://www.meetup.com/rust-berlin/events/000001/)\n",
            "* 2024-10-25 | London, UK | [Rust London](https://www.meetup.com/rust-london/)\n",
            "    * [**Valid Event**](https://www.meetup.com/rust-london/events/000002/)\n",
            "\n",
        );

        let text = build_event_section(Some(extra));
        let (section, line_num) = find_events_section(&text).unwrap();
        let reader = Reader::new(section, line_num);
        let mut linter = EventLinter::new(20);
        let result = linter.lint(reader);

        assert_eq!(result, Err(LintError::ValidationFailed));
        assert_eq!(
            linter.error_count, 1,
            "should have exactly 1 error, not a cascade"
        );
    }

    #[test]
    fn empty_region_does_not_cascade() {
        let extra = concat!(
            "### Asia\n",
            "\n",
            "### Europe\n",
            "* 2024-10-25 | Berlin, DE | [Rust Berlin](https://www.meetup.com/rust-berlin/)\n",
            "    * [**Valid Event**](https://www.meetup.com/rust-berlin/events/000001/)\n",
            "\n",
        );

        let text = build_event_section(Some(extra));
        let (section, line_num) = find_events_section(&text).unwrap();
        let reader = Reader::new(section, line_num);
        let mut linter = EventLinter::new(20);
        let result = linter.lint(reader);

        assert_eq!(result, Err(LintError::ValidationFailed));
        assert_eq!(
            linter.error_count, 1,
            "should have exactly 1 error, not a cascade"
        );
        assert_eq!(linter.state, LinterState::ExpectingRegionHeader);
        assert_eq!(linter.previous_overview, None);
        assert_eq!(linter.pending_overview, None);
        assert!(!linter.region_has_events);
    }

    #[test]
    fn duplicate_event_does_not_cascade() {
        let extra = concat!(
            "### Europe\n",
            "* 2024-10-25 | Berlin, DE | [Rust Berlin](https://www.meetup.com/rust-berlin/)\n",
            "    * [**Duplicate Event**](https://www.meetup.com/rust-berlin/events/000001/)\n",
            "* 2024-10-26 | Berlin, DE | [Rust Berlin](https://www.meetup.com/rust-berlin/)\n",
            "    * [**Duplicate Event**](https://www.meetup.com/rust-berlin/events/000001/)\n",
            "\n",
        );

        let text = build_event_section(Some(extra));
        let (section, line_num) = find_events_section(&text).unwrap();
        let reader = Reader::new(section, line_num);
        let mut linter = EventLinter::new(20);
        let result = linter.lint(reader);

        assert_eq!(result, Err(LintError::ValidationFailed));
        assert_eq!(
            linter.error_count, 1,
            "should have exactly 1 error, not a cascade"
        );
        assert_eq!(linter.state, LinterState::ExpectingRegionHeader);
    }

    #[test]
    fn missing_links_before_next_overview_does_not_cascade() {
        let extra = concat!(
            "### Europe\n",
            "* 2024-10-25 | Berlin, DE | [Rust Berlin](https://example.com/berlin/)\n",
            "* 2024-10-26 | Paris, FR | [Rust Paris](https://example.com/paris/)\n",
            "    * [**Valid Event**](https://example.com/events/valid/)\n",
            "\n",
        );

        let (result, linter) = lint_document(&build_event_section(Some(extra)));

        assert_eq!(result, Err(LintError::ValidationFailed));
        assert_eq!(linter.error_count, 1);
        assert_eq!(event_count(&linter), 2);
        assert_eq!(linter.state, LinterState::ExpectingRegionHeader);
    }

    #[test]
    fn missing_links_before_next_region_does_not_cascade() {
        let extra = concat!(
            "### Europe\n",
            "* 2024-10-25 | Berlin, DE | [Rust Berlin](https://example.com/berlin/)\n",
            "### Asia\n",
            "* 2024-10-26 | Tokyo, JP | [Rust Tokyo](https://example.com/tokyo/)\n",
            "    * [**Valid Event**](https://example.com/events/valid/)\n",
            "\n",
        );

        let (result, linter) = lint_document(&build_event_section(Some(extra)));

        assert_eq!(result, Err(LintError::ValidationFailed));
        assert_eq!(linter.error_count, 1);
        assert_eq!(event_count(&linter), 2);
        assert_eq!(linter.state, LinterState::ExpectingRegionHeader);
    }

    #[test]
    fn malformed_links_abandon_the_pending_event() {
        let extra = concat!(
            "### Europe\n",
            "* 2024-10-25 | Berlin, DE | [Rust Berlin](https://example.com/berlin/)\n",
            "    * [Event is not bold](https://example.com/events/malformed/)\n",
            "* 2024-10-26 | Paris, FR | [Rust Paris](https://example.com/paris/)\n",
            "    * [**Valid Event**](https://example.com/events/valid/)\n",
            "\n",
        );

        let (result, linter) = lint_document(&build_event_section(Some(extra)));

        assert_eq!(result, Err(LintError::ValidationFailed));
        assert_eq!(linter.error_count, 1);
        assert_eq!(event_count(&linter), 2);
        assert_eq!(linter.state, LinterState::ExpectingRegionHeader);
    }

    #[test]
    fn malformed_overview_and_orphaned_links_do_not_cascade() {
        let extra = concat!(
            "### Europe\n",
            "* malformed event overview\n",
            "    * [**Orphaned Event**](https://example.com/events/orphaned/)\n",
            "* 2024-10-26 | Paris, FR | [Rust Paris](https://example.com/paris/)\n",
            "    * [**Valid Event**](https://example.com/events/valid/)\n",
            "\n",
        );

        let (result, linter) = lint_document(&build_event_section(Some(extra)));

        assert_eq!(result, Err(LintError::ValidationFailed));
        assert_eq!(linter.error_count, 2);
        assert_eq!(event_count(&linter), 2);
        assert_eq!(linter.state, LinterState::ExpectingRegionHeader);
    }

    #[test]
    fn newline_while_waiting_for_links_ends_the_region() {
        let extra = concat!(
            "### Europe\n",
            "* 2024-10-25 | Berlin, DE | [Rust Berlin](https://example.com/berlin/)\n",
            "\n",
            "### Asia\n",
            "* 2024-10-26 | Tokyo, JP | [Rust Tokyo](https://example.com/tokyo/)\n",
            "    * [**Valid Event**](https://example.com/events/valid/)\n",
            "\n",
        );

        let (result, linter) = lint_document(&build_event_section(Some(extra)));

        assert_eq!(result, Err(LintError::ValidationFailed));
        assert_eq!(linter.error_count, 1);
        assert_eq!(event_count(&linter), 2);
        assert_eq!(linter.state, LinterState::ExpectingRegionHeader);
    }

    #[test]
    fn valid_fixtures_pass() {
        let _ = simple_logger::init_with_level(log::Level::Error);

        let fixtures = collect_fixtures("test/valid");
        assert!(!fixtures.is_empty(), "no valid fixtures found");

        for path in fixtures {
            let path_clone = path.clone();
            let result = std::panic::catch_unwind(|| lint_file(&path_clone));

            match result {
                Ok(Ok(())) => {} // passed
                Ok(Err(e)) => panic!("{}: lint error: {}", path.display(), e),
                Err(e) => panic!("{}: panicked: {:?}", path.display(), e),
            }
        }
    }

    #[test]
    fn invalid_fixtures_fail() {
        let fixtures = collect_fixtures("test/invalid");
        assert!(!fixtures.is_empty(), "no invalid fixtures found");

        for path in fixtures {
            let result = lint_file(&path);
            assert!(
                result.is_err(),
                "expected {} to fail, but it passed",
                path.display()
            );
        }
    }
}
