use std::sync::LazyLock;

use regex::Regex;

use crate::constants::*;

/// Unwrap message when compiling regexes
const REGEX_FAIL: &str = "Failed to compile regex!";

/// Regex for grabbing timestamps - we use chrono to parse this and do the actual validation
const DATE_RE_STR: &str = r"\d{4}-\d{1,2}-\d{1,2}";

/// Regex capture group names
pub(crate) const START_DATE: &str = "start_date";
pub(crate) const END_DATE: &str = "end_date";
pub(crate) const DATE: &str = "date";
pub(crate) const LOCATION: &str = "location";
pub(crate) const GROUP_URLS: &str = "group_urls";
pub(crate) const LINK_LABEL: &str = "link_label";
pub(crate) const LINK: &str = "link";

/// Regex for extracting newsletter date range, e.g. "Rusty Events between 2024-10-23 - 2024-11-20 🦀"
pub(crate) static EVENT_DATE_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"{} (?<{}>{}) - (?<{}>{})",
        EVENTS_DATE_RANGE_HINT, START_DATE, DATE_RE_STR, END_DATE, DATE_RE_STR
    ))
    .expect(REGEX_FAIL)
});

/// Regex for event date location line hint
pub(crate) static EVENT_DATE_LOCATION_HINT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\* {}", DATE_RE_STR)).expect(REGEX_FAIL));
/// Regex for event date location lines, e.g. "* 2024-10-24 | Virtual | [Women in Rust](https://www.meetup.com/women-in-rust/)"
pub(crate) static EVENT_DATE_LOCATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"\* (?<{}>{}) \| (?<{}>.+) \| (?<{}>.+)",
        DATE, DATE_RE_STR, LOCATION, GROUP_URLS
    ))
    .expect(REGEX_FAIL)
});

/// Delimiter in lines like the following:
///  * 2024-10-24 | Virtual (Berlin, DE) | [OpenTechSchool Berlin](https://berline.rs/) + [Rust Berlin](https://www.meetup.com/rust-berlin/)
pub(crate) const EVENT_DATE_LOCATION_LINK_DELIM: &str = " + ";
/// Delimiter for multiple event links like:
///     * [**Rust Hack and Learn**](https://meet.jit.si/RustHackAndLearnBerlin) | [**Mirror: Rust Hack n Learn Meetup**](https://www.meetup.com/rust-berlin/events/298633271/)
pub(crate) const EVENT_NAME_LINK_DELIM: &str = " | ";

/// Regex for event names, e.g. "* [**Part 4 of 4 - Hackathon Showcase: Final Projects and Presentations**](https..."
pub(crate) static EVENT_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"    \* (.+)").expect(REGEX_FAIL));

/// Regex for validating a markdown link like "[some link](https://www.rust-lang.org/)", this is meant to be very strict and it
/// captures the url as the capture group
pub(crate) static MD_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        // wow! unreadable!
        r"^\[(?<{}>[^\]]+)\]\((?<{}>[^\)]+)\)$",
        LINK_LABEL, LINK,
    ))
    .expect(REGEX_FAIL)
});
