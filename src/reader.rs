use core::fmt;
use std::{borrow::Cow, str::FromStr};

use chrono::NaiveDate;
use nom::{
    bytes::complete::{tag, take_until, take_while1},
    character::complete::char,
    combinator::opt,
    sequence::delimited,
    Parser,
};
use url::Url;

/// Regional headers for events
const REGION_HEADERS: [&str; 7] = [
    "### Virtual",
    "### Africa",
    "### Asia",
    "### Europe",
    "### North America",
    "### Oceania",
    "### South America",
];

/// A markdown formatted link, like "[My Label](https://google.com)"
#[derive(Debug)]
struct MarkdownLink {
    label: String,
    url: Url,
}

/// Parsed event date, can be from a single date like "2025-08-03" or a date range like "2025-08-03 - 2025-08-05"
#[derive(Clone, Debug)]
pub enum EventDate {
    Date(NaiveDate),
    DateRange { start: NaiveDate, end: NaiveDate },
}

impl fmt::Display for EventDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventDate::Date(date) => write!(f, "{}", date),
            EventDate::DateRange { start, end } => write!(f, "{} - {}", start, end),
        }
    }
}

/// Parsed event location, from things like "Virtual", "Virtual (Seattle, WA, US)", "Stockholm, SE", etc.
#[derive(Clone, Debug)]
pub enum EventLocation {
    Virtual,
    // TODO: make an actual location type for more validation
    VirtualWithLocation(String),
    Hybrid(String),
    InPerson(String),
}

impl fmt::Display for EventLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventLocation::Virtual => f.write_str("Virtual"),
            EventLocation::VirtualWithLocation(location) => write!(f, "Virtual ({})", location),
            EventLocation::Hybrid(location) => write!(f, "Hybrid ({})", location),
            EventLocation::InPerson(location) => write!(f, "{}", location),
        }
    }
}

/// The group organizing the event with a link to their homepage, from things like "[Rust Nurnberg DE](https://www.meetup.com/rust-noris/)"
#[derive(Clone, Debug)]
pub struct EventGroup {
    name: String,
    url: Url,
}

impl fmt::Display for EventGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]({})", self.name, self.url)
    }
}

impl From<MarkdownLink> for EventGroup {
    fn from(value: MarkdownLink) -> Self {
        Self {
            name: value.label,
            url: value.url,
        }
    }
}

/// The actual event title and link to information specific to that event, from things like:
/// "    * [**Rust Nürnberg online**](https://www.meetup.com/rust-noris/events/300820274/)"
#[derive(Clone, Debug)]
pub struct Event {
    name: String,
    url: Url,
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[**{}**]({})", self.name, self.url)
    }
}

impl From<MarkdownLink> for Event {
    fn from(value: MarkdownLink) -> Self {
        Self {
            name: value.label,
            url: value.url,
        }
    }
}

/// A full parsed event with all information from the events section, e.g.:
/// "* 2024-10-29 | Aarhus, DK | [Rust Aarhus](https://www.meetup.com/rust-aarhus/)"
/// "   * [**Hack Night**](https://www.meetup.com/rust-aarhus/events/303479865)"
/// An event can have multiple groups hosting it and multiple links to the same event
#[derive(Clone, Debug)]
pub struct EventListing {
    date: EventDate,
    location: EventLocation,
    event_groups: Vec<EventGroup>,
    event_instances: Vec<Event>,
}

impl fmt::Display for EventListing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

/// High level error when reading lines from the newsletter
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineError {
    error: LineParseError,
    num: u64,
    raw: String,
}

impl std::fmt::Display for LineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "parse error: {}\nline #{}: '{}'",
            self.error, self.num, self.raw
        )
    }
}

impl std::error::Error for LineError {}

/// A single line from the newsletter with its parsed representation
/// Contains context useful for debugging if needed (like line number, raw line contents)
#[derive(Debug)]
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

    pub fn num(&self) -> u64 {
        self.line_num
    }

    pub fn parsed(&self) -> &ParsedLine {
        &self.line_parsed
    }

    pub fn raw(&self) -> &Cow<'a, str> {
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

/// An error when attempting to parse a raw line
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineParseError {
    InvalidDate(chrono::format::ParseError),
    InvalidUrl(url::ParseError),
    ParseFailed(String),
}

impl From<chrono::format::ParseError> for LineParseError {
    fn from(value: chrono::format::ParseError) -> Self {
        Self::InvalidDate(value)
    }
}

impl From<url::ParseError> for LineParseError {
    fn from(value: url::ParseError) -> Self {
        Self::InvalidUrl(value)
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

impl fmt::Display for LineParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LineParseError::InvalidDate(e) => write!(f, "invalid date: {}", e),
            LineParseError::InvalidUrl(e) => write!(f, "invalid url: {}", e),
            LineParseError::ParseFailed(e) => write!(f, "failed to parse line: {}", e),
        }
    }
}

impl std::error::Error for LineParseError {}

/// A parsed line, these are the lines we expect to see in the event section, lines in other sections will probably fail to parse
/// in most situations
#[derive(Clone, Debug)]
pub enum ParsedLine {
    /// A newline
    Newline,
    /// Start of the events section, "## Upcoming Events"
    StartEventSection,
    /// The date range in the events section, "Rusty Events between..."
    EventsDateRange { start: NaiveDate, end: NaiveDate },
    /// Header of a section, we use these for the regions, like "### Virtual", "### Asia"...
    RegionHeader(String),
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

    /// Entry point for parsing event lines
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

        // TODO: move to nom tag to be consistent
        if let Some(s) = s.strip_prefix("Rusty Events between ") {
            let (s, start) = parse_date(s)?;
            let (s, _) = tag(" - ")(s)?;
            let (_, end) = parse_date(s)?;

            return Ok(Self::EventsDateRange { start, end });
        }

        // TODO: refactor to be more nom-like
        if REGION_HEADERS.contains(&s) {
            let (_, region) = tag("### ")(s)?;
            return Ok(Self::RegionHeader(region.to_owned()));
        }

        if let (s, Some(_)) = opt(tag("* ")).parse(s)? {
            // parsing as EventOverview, looks something like:
            // "* 2024-10-23 | Austin, TX, US | [Rust ATX](https://www.meetup.com/rust-atx/)"
            let (s, date) = parse_event_date(s)?;
            let (s, _) = tag(" | ")(s)?;

            let (s, location) = parse_location(s)?;
            let (s, _) = tag(" | ")(s)?;

            let mut links = Vec::new();
            let (s, link) = parse_md_link(s)?;
            links.push(link);

            // FIXME: is this right?
            while let (s, Some(_)) = opt(tag(" + ")).parse(s)? {
                let (s, link) = parse_md_link(s)?;
                links.push(link);
            }

            let groups: Vec<EventGroup> = links.into_iter().map(|l| l.into()).collect();

            return Ok(Self::EventOverview {
                date,
                location,
                groups,
            });
        }

        // TODO: what do multiple links here look like? i forget
        if let (s, Some(_)) = opt(tag("    * ")).parse(s)? {
            // parsing as EventLinks, looks like:
            // "    * [**Ferris' Fika Forum #6**](https://www.meetup.com/stockholm-rust/events/303918943/)"
            let (s, link) = parse_md_link(s)?;

            // TODO: maybe find a better place for this?
            if !link.label.starts_with("**") || !link.label.ends_with("**") {
                return Err(LineParseError::ParseFailed(
                    "event link is not bold".to_owned(),
                ));
            }

            return Ok(Self::EventLinks {
                events: vec![link.into()],
            });
        }

        Err(LineParseError::ParseFailed(format!(
            "failed to parse: {}",
            s
        )))
    }
}

impl fmt::Display for ParsedLine {
    // TODO: finish
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ParsedLine::Newline => "Newline",
            ParsedLine::StartEventSection => "StartEventsSection",
            ParsedLine::EventsDateRange { start, end } => {
                &format!("EventsDateRange ({} - {})", start, end)
            }
            ParsedLine::RegionHeader(region) => &format!("RegionHeader ({})", region),
            ParsedLine::EventOverview {
                date,
                location,
                groups,
            } => todo!(),
            ParsedLine::EventLinks { events } => todo!(),
            ParsedLine::EndEventSection => todo!(),
        };
        write!(f, "{}", s)
    }
}

/// Parse a date like "2024-10-24"
fn parse_date(input: &str) -> Result<(&str, NaiveDate), LineParseError> {
    let (input, date) = take_while1(|c: char| c.is_ascii_digit() || c == '-')(input)?;
    let date = date.parse::<NaiveDate>()?;
    Ok((input, date))
}

/// Parse an EventDate that can either be a single date like "2024-10-24", or a range like "2024-10-24 - 2024-10-27"
fn parse_event_date(input: &str) -> Result<(&str, EventDate), LineParseError> {
    let (input, start) = parse_date(input)?;

    if let (input, Some(_)) = opt(tag(" - ")).parse(input)? {
        let (input, end) = parse_date(input)?;
        return Ok((input, EventDate::DateRange { start, end }));
    }

    Ok((input, EventDate::Date(start)))
}

/// Parse an EventLocation like "Virtual", "Virtual (Seattle, WA, US)", "Hamburg, DE", etc.
fn parse_location(input: &str) -> Result<(&str, EventLocation), LineParseError> {
    let mut location_in_parens = delimited(char('('), take_until(")"), char(')'));

    // virtual events first, we expect them either with our without dates, like "Virtual" or "Virtual (Berlin, DE)"
    if let (input, Some(_)) = opt(tag("Virtual")).parse(input)? {
        let (input, location) = opt(location_in_parens).parse(input)?;
        return match location {
            Some(loc) => Ok((input, EventLocation::VirtualWithLocation(loc.to_owned()))),
            None => Ok((input, EventLocation::Virtual)),
        };
    }

    // hybrid events, expect them like "Hybrid (Berlin, DE)"
    if let (input, Some(_)) = opt(tag("Hybrid")).parse(input)? {
        let (input, location) = location_in_parens.parse(input)?;
        return Ok((input, EventLocation::Hybrid(location.to_owned())));
    }

    // otherwise the event is just in person, so take everything up to the pipe delimiter
    let (input, location) = take_until(" |")(input)?;

    Ok((input, EventLocation::InPerson(location.to_owned())))
}

/// Parse a markdown link, like "[Rust ATX](https://www.meetup.com/rust-atx/)"
fn parse_md_link(input: &str) -> Result<(&str, MarkdownLink), LineParseError> {
    let (input, label) = delimited(char('['), take_until("]"), char(']')).parse(input)?;

    // TODO: handle parens in urls properly, this will break currently
    let (input, url) = delimited(char('('), take_until(")"), char(')')).parse(input)?;
    let url = Url::parse(url)?;

    Ok((
        input,
        MarkdownLink {
            label: label.to_owned(),
            url,
        },
    ))
}

/// An iterator over the newsletter, reads each line one by one and attempts to parse it into one of the parsed types we care about
/// for the event section
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
                num: self.current_line_num,
                raw: line.to_owned(),
            }),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_newline() {
        // `lines()` strips newlines for us, so an empty string == newline
        let line = "";
        let parsed = line.parse::<ParsedLine>().unwrap();
        // assert_eq!(parsed, ParsedLine::Newline);
    }
}
