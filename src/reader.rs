use core::fmt;
use std::{borrow::Cow, str::FromStr};

use chrono::NaiveDate;
use log::debug;
use nom::{
    Parser,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::char,
    combinator::opt,
    sequence::delimited,
};
use url::Url;

/// Regional headers for events
// TODO: fix these and remove md header part
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
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EventDate {
    Date(NaiveDate),
    DateRange { start: NaiveDate, end: NaiveDate },
}

impl fmt::Display for EventDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventDate::Date(date) => write!(f, "{date}"),
            EventDate::DateRange { start, end } => write!(f, "{start} - {end}"),
        }
    }
}

/// Parsed event location, from things like "Virtual", "Virtual (Seattle, WA, US)", "Stockholm, SE", etc.
#[derive(Clone, Debug, PartialEq, Eq)]
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
            EventLocation::VirtualWithLocation(location) => write!(f, "Virtual ({location})"),
            EventLocation::Hybrid(location) => write!(f, "Hybrid ({location})"),
            EventLocation::InPerson(location) => write!(f, "{location}"),
        }
    }
}

/// The group organizing the event with a link to their homepage, from things like "[Rust Nurnberg DE](https://www.meetup.com/rust-noris/)"
#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventGroups(Vec<EventGroup>);

impl std::ops::Deref for EventGroups {
    type Target = [EventGroup];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl std::fmt::Display for EventGroups {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = self
            .0
            .iter()
            .map(|eg| eg.to_string())
            .collect::<Vec<_>>()
            .join(" + ");
        write!(f, "{result}")
    }
}

// An event overview line, e.g. "* 2024-10-29 | Aarhus, DK | [Rust Aarhus](https://www.meetup.com/rust-aarhus/)"
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventOverview {
    date: EventDate,
    location: EventLocation,
    groups: EventGroups,
}

impl EventOverview {
    pub fn date(&self) -> &EventDate {
        &self.date
    }

    pub fn location(&self) -> &EventLocation {
        &self.location
    }

    pub fn groups(&self) -> &[EventGroup] {
        &self.groups
    }
}

impl Ord for EventOverview {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // sort by start date if we have a date range
        let date = match self.date {
            EventDate::Date(date) => date,
            EventDate::DateRange { start, end: _ } => start,
        };

        let other_date = match other.date {
            EventDate::Date(date) => date,
            EventDate::DateRange { start, end: _ } => start,
        };

        // first sort by date. if those are equal, use the string representation of the locations
        date.cmp(&other_date)
            .then_with(|| self.location.to_string().cmp(&other.location.to_string()))
    }
}

impl PartialOrd for EventOverview {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for EventOverview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let date = self.date;
        let location = &self.location;
        let groups = &self.groups;

        write!(f, "{date} | {location} | {groups}")
    }
}

/// The actual event title and link to information specific to that event, from things like:
/// "    * [**Rust Nürnberg online**](https://www.meetup.com/rust-noris/events/300820274/)"
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    name: String,
    url: Url,
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // write!(f, "[**{}**]({})", self.name, self.url)
        write!(f, "[{}]({})", self.name, self.url)
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Events(Vec<Event>);

impl std::ops::Deref for Events {
    type Target = [Event];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl fmt::Display for Events {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = self
            .0
            .iter()
            .map(|eg| eg.to_string())
            .collect::<Vec<_>>()
            .join(" + ");
        write!(f, "{result}")
    }
}

/// A full parsed event with all information from the events section, e.g.:
/// "* 2024-10-29 | Aarhus, DK | [Rust Aarhus](https://www.meetup.com/rust-aarhus/)"
/// "   * [**Hack Night**](https://www.meetup.com/rust-aarhus/events/303479865)"
/// An event can have multiple groups hosting it and multiple links to the same event
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventListing {
    overview: EventOverview,
    events: Events,
}

impl fmt::Display for EventListing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut formatted = "* ".to_owned();
        formatted.push_str(&self.overview.to_string());
        formatted.push('\n');
        formatted.push_str("    * ");
        formatted.push_str(&self.events.to_string());
        formatted.push('\n');

        write!(f, "{formatted}")
    }
}

impl From<(EventOverview, Events)> for EventListing {
    fn from(value: (EventOverview, Events)) -> Self {
        Self {
            overview: value.0,
            events: value.1,
        }
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line<'a> {
    line_num: u64,
    line_parsed: ParsedLine,
    line_raw: Cow<'a, str>,
}

impl<'a> Line<'a> {
    pub fn to_owned(&self) -> Line<'static> {
        Line {
            line_num: self.line_num,
            line_parsed: self.line_parsed.clone(),
            line_raw: Cow::Owned(self.line_raw.clone().into_owned()),
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
                Self::ParseFailed(format!("nom failed to parse: {e}"))
            }
            nom::Err::Incomplete(_) => Self::ParseFailed("incomplete input".to_string()),
        }
    }
}

impl fmt::Display for LineParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LineParseError::InvalidDate(e) => write!(f, "invalid date: {e}"),
            LineParseError::InvalidUrl(e) => write!(f, "invalid url: {e}"),
            LineParseError::ParseFailed(e) => write!(f, "failed to parse line: {e}"),
        }
    }
}

impl std::error::Error for LineParseError {}

/// A parsed line, these are the lines we expect to see in the event section, lines in other sections will probably fail to parse
/// in most situations
#[derive(Clone, Debug, PartialEq, Eq)]
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
    EventOverview(EventOverview),
    /// Event name and link to specific event " * [**Part 4 of 4 - Hackathon Showcase: Final Projects and Presentations**]..."
    EventLinks(Events),
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

        // TODO: move to nom tag to be consistent
        if let Some(s) = s.strip_prefix("Rusty Events between ") {
            let (s, start) = parse_date(s)?;
            let (s, _) = tag(" - ")(s)?;
            let (_, end) = parse_date(s)?;

            return Ok(Self::EventsDateRange { start, end });
        }

        // TODO: refactor to be more nom-like
        if REGION_HEADERS.contains(&s) {
            let (region, _) = tag("### ")(s)?;
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
            let mut remaining = s;
            loop {
                let (s, tag) = opt(tag(" + ")).parse(remaining)?;

                if tag.is_some() {
                    let (s, link) = parse_md_link(s)?;
                    remaining = s;
                    links.push(link);
                } else {
                    break;
                }
            }

            let groups: Vec<EventGroup> = links.into_iter().map(|l| l.into()).collect();
            let groups = EventGroups(groups);

            let overview = EventOverview {
                date,
                location,
                groups,
            };
            return Ok(Self::EventOverview(overview));
        }

        // TODO: what do multiple links here look like? i forget
        if let (s, Some(_)) = opt(tag("    * ")).parse(s)? {
            // parsing as EventLinks, looks like:
            // "    * [**Ferris' Fika Forum #6**](https://www.meetup.com/stockholm-rust/events/303918943/)"
            let (_, link) = parse_md_link(s)?;

            // TODO: maybe find a better place for this?
            if !link.label.starts_with("**") || !link.label.ends_with("**") {
                return Err(LineParseError::ParseFailed(
                    "event link is not bold".to_owned(),
                ));
            }

            return Ok(Self::EventLinks(Events(vec![link.into()])));
        }

        Err(LineParseError::ParseFailed(
            format!("failed to parse: {s}",),
        ))
    }
}

impl fmt::Display for ParsedLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ParsedLine::Newline => "Newline",
            ParsedLine::StartEventSection => "StartEventsSection",
            ParsedLine::EventsDateRange { start, end } => {
                &format!("EventsDateRange ({start} - {end})")
            }
            ParsedLine::RegionHeader(region) => &format!("RegionHeader ({region})"),
            ParsedLine::EventOverview(overview) => &format!("EventOverview ({overview})"),
            ParsedLine::EventLinks(events) => &format!("EventLinks ({events})"),
        };
        write!(f, "{s}")
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
    let mut location_in_parens = delimited(tag(" ("), take_until(")"), char(')'));

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
        // TODO: pull this string out into a const somewhere as we reference it more than once
        let events_start = contents
            .find("## Upcoming Events")
            .expect("no events section header found");
        let current_line_num = contents[..events_start].lines().count() as u64;
        let contents = &contents[events_start..];

        let events_end = contents
            .find("If you are running a Rust event please add it to the [calendar] to get")
            .expect("no events section end found");
        let contents = &contents[..events_end];

        Self {
            contents,
            current_line_num,
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
        assert_eq!(parsed, ParsedLine::Newline);
    }
}
