use std::sync::LazyLock;

use url::Host;

/// Lines we expect to match exactly
pub(crate) const START_EVENTS_SECTION: &str = "## Upcoming Events";
pub(crate) const MD_HEADER: &str = "#";
pub(crate) const EVENT_REGION_HEADER: &str = "### ";
pub(crate) const END_EVENTS_SECTION: &str =
    "If you are running a Rust event please add it to the [calendar]";

/// Hints for what type of line we are parsing - this helps us generate a bit better error messages
pub(crate) const EVENTS_DATE_RANGE_HINT: &str = "Rusty Events between";
pub(crate) const EVENT_NAME_HINT: &str = "    * [**";

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
pub const REGIONS: [&str; 7] = [
    "Virtual",
    "Africa",
    "Asia",
    "Europe",
    "North America",
    "Oceania",
    "South America",
];

/// The meetup.com domain (needs String, hence the LazyLock)
pub(crate) static MEETUP_DOMAIN: LazyLock<Host> =
    LazyLock::new(|| Host::Domain("www.meetup.com".to_owned()));
/// The tracker that is sometimes included in the meetup urls
pub(crate) const MEETUP_TRACKER: &str = "eventOrigin";
