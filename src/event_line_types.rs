use std::{fmt, str::FromStr};

use chrono::{NaiveDate, ParseError};
use log::{debug, warn};
use regex::Regex;
use url::Url;

use crate::{constants::*, lint::LintError, regex::*};

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
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
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
                let (date, location) = Self::extract_and_validate_date_location_group(s)?;
                Self::EventDateLocationGroup(EventDateLocation {
                    date,
                    location: location.to_owned(),
                })
            }
            s if s.starts_with(EVENT_NAME_HINT) => {
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
    /// Helper for regex errors
    fn map_regex_error(regex: &Regex) -> LintError {
        LintError::RegexError {
            regex_string: regex.as_str().to_owned(),
        }
    }

    /// Helper for chrono parse errors
    fn map_chrono_parse_error(chrono_error: ParseError) -> LintError {
        LintError::DateParseError { chrono_error }
    }

    /// Extracts date range for the newletter, these are used to validate events fall within the given date range
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

    /// Extracts date and location from events, also validates group links
    fn extract_and_validate_date_location_group(
        line: &str,
    ) -> Result<(NaiveDate, &str), LintError> {
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
        // TODO: validate location formatting

        let date_parsed = date_capture
            .parse::<NaiveDate>()
            .map_err(Self::map_chrono_parse_error)?;

        // now we will validate the rest of the line with the group names + links. We may have more than one here as well
        let links_capture = captures
            .name(GROUP_URLS)
            .ok_or_else(|| Self::map_regex_error(re))?
            .as_str();

        // if we have multiple links, we expect them to be delimited with ' + '
        let links: Vec<&str> = if links_capture.contains(EVENT_DATE_LOCATION_LINK_DELIM) {
            links_capture
                .split(EVENT_DATE_LOCATION_LINK_DELIM)
                .collect()
        } else {
            vec![links_capture]
        };

        Self::validate_markdown_urls(links, false)?;

        Ok((date_parsed, location_capture))
    }

    /// Validates event names/links
    fn validate_event_name(line: &str) -> Result<(), LintError> {
        let re = &*EVENT_NAME_RE;
        let captures = re.captures(line).ok_or_else(|| Self::map_regex_error(re))?;
        debug!("Captured: '{:?}'", &captures);

        let link_captures = captures
            .get(1)
            .ok_or_else(|| Self::map_regex_error(re))?
            .as_str();

        // multiple links here should be ' | ' delimited
        let links: Vec<&str> = if link_captures.contains(EVENT_NAME_LINK_DELIM) {
            link_captures.split(EVENT_NAME_LINK_DELIM).collect()
        } else {
            vec![link_captures]
        };

        Self::validate_markdown_urls(links, true)?;

        Ok(())
    }

    /// Validates one or more links are formatted as expected in markdown, e.g. `[My label](https://mylink.test)`
    // TODO: don't like bool args, clean this up probably. Ok for now since this check is so simple and all the code that
    // calls this function is right here
    fn validate_markdown_urls(urls: Vec<&str>, check_label_is_bold: bool) -> Result<(), LintError> {
        let re = &*MD_LINK_RE;
        for url in urls {
            let capture = re.captures(url).ok_or_else(|| LintError::RegexError {
                regex_string: re.as_str().to_owned(),
            })?;

            debug!("Captured: '{:?}'", &capture);

            let label = capture
                .name(LINK_LABEL)
                .ok_or_else(|| LintError::RegexError {
                    regex_string: re.as_str().to_owned(),
                })?
                .as_str();

            if check_label_is_bold
                && (&label[0..2] != "**" || &label[label.len() - 2..label.len()] != "**")
            {
                return Err(LintError::InvalidLinkLabel(label.to_owned()));
            }

            let url = capture
                .name(LINK)
                .ok_or_else(|| LintError::RegexError {
                    regex_string: re.as_str().to_owned(),
                })?
                .as_str();

            Self::validate_url(&Url::parse(url).map_err(LintError::InvalidUrl)?)?;
        }

        Ok(())
    }

    /// Validates a URL is actually kind of valid and any domain-specific logic can be implemented here
    fn validate_url(url: &Url) -> Result<(), LintError> {
        // TODO: probably make this an error just for better visibility? like getting line # in error message
        if url.scheme() != "https" {
            warn!(
                "Unexpected URL protocol '{}' in url '{}'",
                url.scheme(),
                url
            );
        }

        // domain specific logic
        if let Some(domain) = url.host() {
            // meetup.com
            if domain == *MEETUP_DOMAIN {
                if let Some(query_string) = url.query() {
                    if query_string.contains(MEETUP_TRACKER) {
                        return Err(LintError::UrlContainsTracker(url.clone()));
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_newline() -> TestResult {
        // `lines()` strips newlines for us, so an empty string == newline
        let line = "";
        let parsed = line.parse::<EventLineType>()?;
        assert_eq!(parsed, EventLineType::Newline);
        Ok(())
    }

    #[test]
    fn test_start_events_section() -> TestResult {
        let line = "## Upcoming Events";
        let parsed = line.parse::<EventLineType>()?;
        assert_eq!(parsed, EventLineType::StartEventSection);
        Ok(())
    }

    #[test]
    fn test_events_date_range() -> TestResult {
        let line = "Rusty Events between 2024-10-23 - 2024-11-20 🦀";
        let parsed = line.parse::<EventLineType>()?;

        let expected = EventLineType::EventsDateRange(
            "2024-10-23".parse::<NaiveDate>()?,
            "2024-11-20".parse::<NaiveDate>()?,
        );

        assert_eq!(parsed, expected);
        Ok(())
    }

    #[test]
    fn test_event_region_header() -> TestResult {
        let line = "### Virtual";
        let parsed = line.parse::<EventLineType>()?;
        let expected = EventLineType::EventRegionHeader("Virtual".to_owned());
        assert_eq!(parsed, expected);
        Ok(())
    }

    #[test]
    fn test_event_date_location_group() -> TestResult {
        let line =
            "* 2024-10-24 | Virtual | [Women in Rust](https://www.meetup.com/women-in-rust/)";
        let parsed = line.parse::<EventLineType>()?;

        let expected = EventLineType::EventDateLocationGroup(EventDateLocation {
            date: "2024-10-24".parse::<NaiveDate>()?,
            location: "Virtual".to_owned(),
        });

        assert_eq!(parsed, expected);
        Ok(())
    }

    #[test]
    fn test_event_name() -> TestResult {
        let line = "    * [**Part 4 of 4 - Hackathon Showcase: Final Projects and Presentations**](https://www.meetup.com/women-in-rust/events/303213835/)";
        let parsed = line.parse::<EventLineType>()?;
        assert_eq!(parsed, EventLineType::EventName);
        Ok(())
    }

    #[test]
    fn test_end_event_section() -> TestResult {
        let line = "If you are running a Rust event please add it to the [calendar] to get";
        let parsed = line.parse::<EventLineType>()?;
        assert_eq!(parsed, EventLineType::EndEventSection);
        Ok(())
    }

    #[test]
    fn test_unrecognized() -> TestResult {
        let line = "some line with words and things";
        let parsed = line.parse::<EventLineType>()?;
        assert_eq!(parsed, EventLineType::Unrecognized);
        Ok(())
    }

    #[test]
    fn test_invalid_region_header() -> TestResult {
        let line = "### Pangea";
        let parsed = line.parse::<EventLineType>();
        assert_eq!(parsed, Err(LintError::UnknownRegion("Pangea".to_owned())));
        Ok(())
    }

    #[test]
    fn test_meetup_url_contains_tracker() -> TestResult {
        let line = "    * [**My test link**](https://www.meetup.com/women-in-rust/events/303213835/?eventOrigin=group_events_list)";
        let parsed = line.parse::<EventLineType>();

        let url = Url::from_str(
            "https://www.meetup.com/women-in-rust/events/303213835/?eventOrigin=group_events_list",
        )?;
        assert_eq!(parsed, Err(LintError::UrlContainsTracker(url)));
        Ok(())
    }

    #[test]
    fn test_non_bold_event_name() -> TestResult {
        let line = "    * [**November Meetup*](https://www.meetup.com/join-srug/events/304166747/)";
        let parsed = line.parse::<EventLineType>();

        assert_eq!(
            parsed,
            Err(LintError::InvalidLinkLabel("**November Meetup*".to_owned()))
        );
        Ok(())
    }
}
