use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use chrono::NaiveDate;
use serde::{Deserialize, Deserializer};
use url::Url;

const VIRTUAL: &str = "Virtual";
const AFRICA: &str = "Africa";
const ASIA: &str = "Asia";
const EUROPE: &str = "Europe";
const NORTH_AMERICA: &str = "North America";
const OCEANIA: &str = "Oceania";
const SOUTH_AMERICA: &str = "South America";

/// Regional headers for events
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize)]
pub enum Region {
    Virtual,
    Africa,
    Asia,
    Europe,
    #[serde(rename = "North America")]
    NorthAmerica,
    Oceania,
    #[serde(rename = "South America")]
    SouthAmerica,
}

impl Region {
    pub const ALL: [Region; 7] = [
        Region::Virtual,
        Region::Africa,
        Region::Asia,
        Region::Europe,
        Region::NorthAmerica,
        Region::Oceania,
        Region::SouthAmerica,
    ];
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Virtual => VIRTUAL,
            Self::Africa => AFRICA,
            Self::Asia => ASIA,
            Self::Europe => EUROPE,
            Self::NorthAmerica => NORTH_AMERICA,
            Self::Oceania => OCEANIA,
            Self::SouthAmerica => SOUTH_AMERICA,
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for Region {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            VIRTUAL => Ok(Self::Virtual),
            AFRICA => Ok(Self::Africa),
            ASIA => Ok(Self::Asia),
            EUROPE => Ok(Self::Europe),
            NORTH_AMERICA => Ok(Self::NorthAmerica),
            OCEANIA => Ok(Self::Oceania),
            SOUTH_AMERICA => Ok(Self::SouthAmerica),
            _ => Err(format!("invalid region '{s}'")),
        }
    }
}

/// A markdown formatted link, like "[My Label](https://google.com)"
#[derive(Debug)]
pub struct MarkdownLink {
    label: String,
    url: Url,
}

impl MarkdownLink {
    pub fn new(label: String, url: Url) -> Self {
        Self { label, url }
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Parsed event date, can be from a single date like "2025-08-03" or a date range like "2025-08-03 - 2025-08-05"
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EventDate {
    Date(NaiveDate),
    DateRange { start: NaiveDate, end: NaiveDate },
}

impl std::fmt::Display for EventDate {
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

impl std::fmt::Display for EventLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Virtual => f.write_str("Virtual"),
            Self::VirtualWithLocation(location) => write!(f, "Virtual ({location})"),
            Self::Hybrid(location) => write!(f, "Hybrid ({location})"),
            Self::InPerson(location) => write!(f, "{location}"),
        }
    }
}

/// The group organizing the event with a link to their homepage, from things like "[Rust Nurnberg DE](https://www.meetup.com/rust-noris/)"
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventGroup {
    name: String,
    url: Url,
}

impl std::fmt::Display for EventGroup {
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

impl From<Vec<EventGroup>> for EventGroups {
    fn from(value: Vec<EventGroup>) -> Self {
        Self(value)
    }
}

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
    pub fn new(date: EventDate, location: EventLocation, groups: EventGroups) -> Self {
        Self {
            date,
            location,
            groups,
        }
    }
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

impl std::fmt::Display for EventOverview {
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

impl std::fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: why did i comment this out
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

impl From<Vec<Event>> for Events {
    fn from(value: Vec<Event>) -> Self {
        Self(value)
    }
}

impl std::ops::Deref for Events {
    type Target = [Event];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl std::fmt::Display for Events {
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

/// A fully parsed event with all information from the events section, e.g.:
/// "* 2024-10-29 | Aarhus, DK | [Rust Aarhus](https://www.meetup.com/rust-aarhus/)"
/// "   * [**Hack Night**](https://www.meetup.com/rust-aarhus/events/303479865)"
/// An event can have multiple groups hosting it and multiple links to the same event
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventListing {
    overview: EventOverview,
    events: Events,
}

impl Ord for EventListing {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.overview.cmp(&other.overview)
    }
}

impl PartialOrd for EventListing {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::hash::Hash for EventListing {
    /// Hash the event - this should uniquely identify the event. We use this when reading in new events
    /// to determine if an event is the same or not. We do this because event dates, titles, etc. can change, and
    /// we want to follow the same event if its updated and not duplicate it (with the old event being incorrect)
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.events
            .as_ref()
            .iter()
            .map(|e| e.url.as_str())
            .collect::<Vec<&str>>()
            .hash(state);
    }
}

impl std::fmt::Display for EventListing {
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

// TODO: probably don't need this
impl From<(EventOverview, Events)> for EventListing {
    fn from(value: (EventOverview, Events)) -> Self {
        Self {
            overview: value.0,
            events: value.1,
        }
    }
}

// vibecoded, be aware
impl<'de> Deserialize<'de> for EventListing {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        #[derive(Deserialize)]
        struct JsonEvent {
            name: String,
            location: String,
            date: String,
            url: String,
            #[serde(rename = "virtual")]
            is_virtual: bool,
            organizer_name: String,
            organizer_url: String,
            #[serde(rename = "hybrid")]
            is_hybrid: bool,
        }

        let json_event = JsonEvent::deserialize(deserializer)?;

        let date = NaiveDate::parse_from_str(&json_event.date, "%Y-%m-%d")
            .map_err(|_| Error::custom("invalid date format, expected YYYY-MM-DD"))?;

        let event_url =
            Url::parse(&json_event.url).map_err(|_| Error::custom("invalid event URL"))?;
        let organizer_url = Url::parse(&json_event.organizer_url)
            .map_err(|_| Error::custom("invalid organizer URL"))?;

        let location = if json_event.is_hybrid {
            EventLocation::Hybrid(json_event.location)
        } else if json_event.is_virtual {
            EventLocation::VirtualWithLocation(json_event.location)
        } else {
            EventLocation::InPerson(json_event.location)
        };

        let group = EventGroup {
            name: json_event.organizer_name,
            url: organizer_url,
        };

        let overview = EventOverview {
            date: EventDate::Date(date),
            location,
            groups: EventGroups(vec![group]),
        };

        let event = Event {
            name: json_event.name,
            url: event_url,
        };

        Ok(EventListing {
            overview,
            events: Events(vec![event]),
        })
    }
}

#[derive(Debug)]
pub struct EventsByRegion(HashMap<Region, Vec<EventListing>>);

impl EventsByRegion {
    pub fn new() -> Self {
        EventsByRegion(HashMap::new())
    }

    pub fn add(&mut self, listing: EventListing, region: Region) {
        self.0.entry(region).or_default().push(listing)
    }

    pub fn merge(&self, other: &EventsByRegion) -> Self {
        let mut updated = EventsByRegion::new();

        for region in Region::ALL {
            let maybe_current_events = self.0.get(&region);
            let maybe_new_events = other.0.get(&region);

            // no current events in region, only new. take all new events
            if maybe_current_events.is_none()
                && let Some(new_events) = maybe_new_events
            {
                for event in new_events {
                    updated.add(event.clone(), region);
                }
            }

            // no new events in region, only current. take all current events
            if maybe_new_events.is_none()
                && let Some(current_events) = maybe_current_events
            {
                for event in current_events {
                    updated.add(event.clone(), region);
                }
            }

            // both new and current events - needs merge logic
            if let Some(new_events) = maybe_new_events
                && let Some(current_events) = maybe_current_events
            {
                let new = new_events
                    .iter()
                    .cloned()
                    .collect::<HashSet<EventListing>>();

                let current = current_events
                    .iter()
                    .cloned()
                    .collect::<HashSet<EventListing>>();

                // anything that overlaps, we take the newer version of the event. otherwise copy everything else
                let mut merged = new;
                merged.extend(current.difference(&merged.clone()).cloned());

                for event in merged {
                    updated.add(event.clone(), region);
                }
            }
            // let the case where both are none fall through - nothing to do here
        }

        updated
    }
}

impl Default for EventsByRegion {
    fn default() -> Self {
        Self::new()
    }
}

impl From<HashMap<Region, Vec<EventListing>>> for EventsByRegion {
    fn from(value: HashMap<Region, Vec<EventListing>>) -> Self {
        Self(value)
    }
}

impl<'a> IntoIterator for &'a EventsByRegion {
    type Item = (&'a Region, &'a Vec<EventListing>);
    type IntoIter = std::collections::hash_map::Iter<'a, Region, Vec<EventListing>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'de> Deserialize<'de> for EventsByRegion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // TODO: can we just derive this?
        let regions: HashMap<Region, Vec<EventListing>> = HashMap::deserialize(deserializer)?;
        Ok(regions.into())
    }
}

impl std::fmt::Display for EventsByRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();

        for region in Region::ALL {
            if let Some(events) = self.0.get(&region) {
                // TODO: cleanup
                let mut events = events.clone();
                events.sort();

                s.push_str(&format!("### {region}\n"));

                for event in events {
                    s.push_str(&format!("{event}"));
                }
            }
            s.push('\n');
        }

        write!(f, "{s}")
    }
}
