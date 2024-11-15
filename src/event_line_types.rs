use std::{fmt, str::FromStr, sync::LazyLock};

use chrono::{NaiveDate, ParseError};
use log::debug;
use regex::Regex;
use url::Url;

use crate::lint::LintError;

/// Unwrap message when compiling regexes
const REGEX_FAIL: &str = "Failed to compile regex!";

/// Lines we expect to match exactly
const START_EVENTS_SECTION: &str = "## Upcoming Events";
const EVENT_REGION_HEADER: &str = "### ";
pub(crate) const END_EVENTS_SECTION: &str =
    "If you are running a Rust event please add it to the [calendar]";

/// Hints for what type of line we are parsing - this helps us generate a bit better error messages
const EVENTS_DATE_RANGE_HINT: &str = "Rusty Events between";
const EVENT_NAME_HINT: &str = "    * [**";

/// Regex for grabbing timestamps - we use chrono to parse this and do the actual validation
const DATE_RE_STR: &str = r"\d{4}-\d{1,2}-\d{1,2}";

/// Line "types" in the event section. We use this in several different stringy contexts, so just hardcode the strings here
/// See EventLineType for a description of each type
pub(crate) const NEWLINE_TYPE: &str = "Newline";
pub(crate) const START_EVENT_SECTION_TYPE: &str = "StartEventSection";
pub(crate) const EVENTS_DATE_RANGE_TYPE: &str = "EventsDateRange";
pub(crate) const EVENT_REGION_HEADER_TYPE: &str = "EventRegionHeader";
pub(crate) const EVENT_DATE_LOCATION_GROUP_TYPE: &str = "EventDateLocationGroup";
pub(crate) const EVENT_NAME_TYPE: &str = "EventName";
pub(crate) const END_EVENT_SECTION_TYPE: &str = "EndEventSection";
pub(crate) const UNRECOGNIZED_TYPE: &str = "Unrecognized";

/// Regions from headers, e.g. "Virtual", "Asia", "Europe", etc.
pub(crate) const REGIONS: [&str; 5] = ["Virtual", "Asia", "Europe", "North America", "Oceania"];

/// Regex capture group names
const START_DATE: &str = "start_date";
const END_DATE: &str = "end_date";
const DATE: &str = "date";
const LOCATION: &str = "location";
const GROUP_URLS: &str = "group_urls";

/// Regex for extracting newsletter date range, e.g. "Rusty Events between 2024-10-23 - 2024-11-20 🦀"
static EVENT_DATE_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"{} (?<{}>{}) - (?<{}>{})",
        EVENTS_DATE_RANGE_HINT, START_DATE, DATE_RE_STR, END_DATE, DATE_RE_STR
    ))
    .expect(REGEX_FAIL)
});

/// Regex for event date location line hint
static EVENT_DATE_LOCATION_HINT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\* {}", DATE_RE_STR)).expect(REGEX_FAIL));
/// Regex for event date location lines, e.g. " * 2024-10-24 | Virtual | [Women in Rust](https://www.meetup.com/women-in-rust/)"
static EVENT_DATE_LOCATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"\* (?<{}>{}) \| (?<{}>.+) \| (?<{}>.+)",
        DATE, DATE_RE_STR, LOCATION, GROUP_URLS
    ))
    .expect(REGEX_FAIL)
});

/// Regex for event names, e.g. "* [**Part 4 of 4 - Hackathon Showcase: Final Projects and Presentations**](https..."
static EVENT_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"    \* (.+)").expect(REGEX_FAIL));

// TODO: some lines have multiple links together, should capture all of them
static MD_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[.+\]\((.+)\)").expect(REGEX_FAIL));

/// An event's date and location. Used to ensure our dates are ordered correctly, first by date, then by location
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EventDateLocation {
    date: NaiveDate,
    location: String,
}

impl EventDateLocation {
    pub fn date(&self) -> &NaiveDate {
        &self.date
    }

    pub fn location(&self) -> &str {
        &self.location
    }
}

/// The type of a given line of text in the event section
#[derive(Debug)]
pub(crate) enum EventLineType {
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
            _ if s == START_EVENTS_SECTION => Self::StartEventSection,
            s if s.starts_with(EVENTS_DATE_RANGE_HINT) => {
                let parsed_time_range = Self::extract_date_range(s)?;
                Self::EventsDateRange(parsed_time_range.0, parsed_time_range.1)
            }
            s if s.starts_with(EVENT_REGION_HEADER) => {
                let region = Self::extract_and_validate_region_header(s)?;
                Self::EventRegionHeader(region.to_owned())
            }
            s if EVENT_DATE_LOCATION_HINT_RE.is_match(s) => {
                // TODO: validate
                let (date, location) = Self::extract_date_location_group(s)?;
                Self::EventDateLocationGroup(EventDateLocation {
                    date,
                    location: location.to_owned(),
                })
            }
            s if s.starts_with(EVENT_NAME_HINT) => {
                // TODO: validate
                Self::validate_event_name(s)?;
                Self::EventName
            }
            _ if s.starts_with(END_EVENTS_SECTION) => Self::EndEventSection,
            _ => Self::Unrecognized,
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
    fn map_regex_error(regex: &Regex) -> LintError {
        LintError::RegexError {
            regex_string: regex.as_str().to_owned(),
        }
    }

    fn map_chrono_parse_error(chrono_error: ParseError) -> LintError {
        LintError::DateParseError { chrono_error }
    }

    fn extract_date_range(line: &str) -> Result<(NaiveDate, NaiveDate), LintError> {
        let re = &*EVENT_DATE_RANGE_RE;
        let captures = re.captures(line).ok_or_else(|| Self::map_regex_error(re))?;

        debug!("Captured: '{:?}'", &captures);

        let start_capture = captures
            .name(START_DATE)
            .ok_or_else(|| Self::map_regex_error(re))?
            .as_str();

        let end_capture = captures
            .name(END_DATE)
            .ok_or_else(|| Self::map_regex_error(re))?
            .as_str();

        let start_parsed = start_capture
            .parse::<NaiveDate>()
            .map_err(Self::map_chrono_parse_error)?;

        let end_parsed = end_capture
            .parse::<NaiveDate>()
            .map_err(Self::map_chrono_parse_error)?;

        Ok((start_parsed, end_parsed))
    }

    /// Extracts and validates the region is an expected one in a region header (e.g. "### Virtual")
    fn extract_and_validate_region_header(line: &str) -> Result<&str, LintError> {
        let region = line
            .strip_prefix(EVENT_REGION_HEADER)
            .ok_or(LintError::ParseError)?;

        if !REGIONS.contains(&region) {
            Err(LintError::UnknownRegion(region.to_owned()))
        } else {
            Ok(region)
        }
    }

    fn extract_date_location_group(line: &str) -> Result<(NaiveDate, &str), LintError> {
        let re = &*EVENT_DATE_LOCATION_RE;
        let captures = re.captures(line).ok_or_else(|| Self::map_regex_error(re))?;

        debug!("Captured: '{:?}'", &captures);

        // get our required data, the date and location
        let date_capture = captures
            .name(DATE)
            .ok_or_else(|| Self::map_regex_error(re))?
            .as_str();

        let location_capture = captures
            .name(LOCATION)
            .ok_or_else(|| Self::map_regex_error(re))?
            .as_str();

        let date_parsed = date_capture
            .parse::<NaiveDate>()
            .map_err(Self::map_chrono_parse_error)?;

        // now we will validate the rest of the line with the group names + links. We may have more than one here as well
        let links_capture = captures
            .name(GROUP_URLS)
            .ok_or_else(|| Self::map_regex_error(re))?
            .as_str();

        Self::validate_markdown_url(links_capture)?;

        Ok((date_parsed, location_capture))
    }

    fn validate_event_name(line: &str) -> Result<(), LintError> {
        let re = &*EVENT_NAME_RE;
        let captures = re.captures(line).ok_or_else(|| Self::map_regex_error(re))?;
        debug!("Captured: '{:?}'", &captures);

        let link = captures
            .get(1)
            .ok_or_else(|| Self::map_regex_error(re))?
            .as_str();
        Self::validate_markdown_url(link)?;

        Ok(())
    }

    fn validate_markdown_url(url: &str) -> Result<(), LintError> {
        let re = &*MD_LINK_RE;
        let capture = re.captures(url).ok_or_else(|| LintError::RegexError {
            regex_string: re.as_str().to_owned(),
        })?;

        let url = capture
            .get(1)
            .ok_or_else(|| LintError::RegexError {
                regex_string: re.as_str().to_owned(),
            })?
            .as_str();

        Url::parse(url).map_err(LintError::InvalidUrl)?;

        Ok(())
    }
}
