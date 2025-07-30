use std::{borrow::Cow, collections::HashMap, str::FromStr};

use chrono::{NaiveDate, NaiveWeek};
use nom::{bytes::complete::tag, bytes::complete::take_while1, combinator::map_res, IResult};
use url::Url;

use crate::line_types::{EventLineType, LineParseError};

/// A single line from
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line<'a> {
    line_num: u64,
    line_parsed: ParsedLine,
    line_raw: Cow<'a, str>,
}

impl<'a> Line<'a> {
    pub fn into_owned(self) -> Line<'static> {
        Line {
            line_num: self.line_num,
            line_parsed: self.line_parsed.clone(),
            line_raw: Cow::Owned(self.line_raw.into_owned()),
        }
    }

    pub fn get_line_num(&self) -> u64 {
        self.line_num
    }

    pub fn get_line_type(&self) -> &ParsedLine {
        &self.line_parsed
    }

    pub fn get_line_raw(&self) -> &Cow<'a, str> {
        &self.line_raw
    }
}

impl std::fmt::Display for Line<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line #{}, type '{}': '{}'",
            self.line_num, self.line_parsed, self.line_raw
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineError {
    error: LineParseError,
    line_num: u64,
    line_raw: String,
}

impl std::fmt::Display for LineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "parse error: {}\nline #{}: '{}'",
            self.error, self.line_num, self.line_raw
        )
    }
}

// TODO: where to put nice interface to collect events from?
#[derive(Debug)]
pub struct Reader<'a> {
    contents: &'a str,
    current_line_num: u64,
}

impl<'a> Reader<'a> {
    pub fn new(contents: &'a str) -> Self {
        Self {
            contents,
            current_line_num: 0,
        }
    }
}

impl<'a> Iterator for Reader<'a> {
    type Item = Result<Line<'a>, LineError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.contents.is_empty() {
            return None;
        }

        self.current_line_num += 1;

        let line = match self.contents.find('\n') {
            Some(offset) => {
                let line = &self.contents[..offset];
                // leave out our newline
                self.contents = &self.contents[offset + 1..];
                line
            }
            None => self.contents,
        };

        Some(match line.parse::<ParsedLine>() {
            Ok(line_type) => Ok(Line {
                line_num: self.current_line_num,
                line_parsed: line_type,
                line_raw: Cow::Borrowed(line),
            }),
            Err(e) => Err(LineError {
                error: e,
                line_num: self.current_line_num,
                line_raw: line.to_owned(),
            }),
        })
    }
}

/// TODO: probably move this into its own thing
/// maybe make line_types use this
#[derive(Debug)]
pub enum EventDate {
    Date(NaiveDate),
    DateRange { start: NaiveDate, end: NaiveDate },
}

#[derive(Debug)]
pub enum EventLocation {
    Virtual,
    VirtualWithLocation(String),
    Hybrid(String),
    InPerson,
}

#[derive(Debug)]
pub struct EventGroup {
    name: String,
    url: Url,
}

#[derive(Debug)]
pub struct Event {
    name: String,
    url: Url,
}

#[derive(Debug)]
pub struct EventListing {
    date: EventDate,
    location: EventLocation,
    event_groups: Vec<EventGroup>,
    event_instances: Vec<Event>,
}

const REGION_HEADERS: [&str; 7] = [
    "### Virtual",
    "### Africa",
    "### Asia",
    "### Europe",
    "### North America",
    "### Oceania",
    "### South America",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineParseError {
    PatternNotMatched(String),
    InvalidDate(chrono::format::ParseError),
    InvalidUrl(url::ParseError),
    UnknownRegion(String),
    InvalidLinkLabel(String),
    UrlContainsTracker(Url),
    ParseFailed(String),
}

impl From<chrono::format::ParseError> for LineParseError {
    fn from(value: chrono::format::ParseError) -> Self {
        Self::InvalidDate(value)
    }
}

impl From<nom::Err<nom::error::Error<&str>>> for LineParseError {
    fn from(value: nom::Err<nom::error::Error<&str>>) -> Self {
        match value {
            nom::Err::Error(e) | nom::Err::Failure(e) => {
                Self::ParseFailed(format!("failed to parse: {}", e))
            }
            nom::Err::Incomplete(_) => Self::ParseFailed("incomplete input".to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ParsedLine {
    /// A newline
    Newline,
    /// Start of the events section, "## Upcoming Events"
    StartEventSection,
    /// The date range in the events section, "Rusty Events between..."
    EventsDateRange { start: NaiveDate, end: NaiveDate },
    /// Header of a section, we use these for the regions, like "### Virtual", "### Asia"...
    RegionHeader(Region),
    /// First line of an event with the date, location, and group link "* 2024-10-24 | Virtual | [Women in Rust]..."
    EventOverview {
        date: EventDate,
        location: EventLocation,
        groups: Vec<EventGroup>,
    },
    /// Event name and link to specific event " * [**Part 4 of 4 - Hackathon Showcase: Final Projects and Presentations**]..."
    EventLinks { events: Vec<Event> },
    /// End of the event section "If you are running a Rust event please add..."
    EndEventSection,
}

impl FromStr for ParsedLine {
    type Err = LineParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Ok(Self::Newline);
        }

        if s == "## Upcoming Events" {
            return Ok(Self::StartEventSection);
        }

        if s == "If you are running a Rust event please add it to the [calendar] to get" {
            return Ok(Self::EndEventSection);
        }

        if let Some(s) = s.strip_prefix("Rusty Events between ") {
            // TODO: figure out nom error types a bit better so we don't have to swallow the error here
            let (s, start) = extract_date(s)?;
            let start = start.parse::<NaiveDate>()?;

            let (s, _) = tag(" - ")(s)?;

            let (_, end) = extract_date(s)?;
            let end = end.parse::<NaiveDate>()?;

            return Ok(Self::EventsDateRange { start, end });
        }

        if REGION_HEADERS.contains(&s) {
            let (_, region) = tag("### ")(s)?;
            return Ok(Self::RegionHeader(region.to_owned()));
        }

        todo!()
    }
}

fn extract_date(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_ascii_digit() || c == '-')(input)
}
