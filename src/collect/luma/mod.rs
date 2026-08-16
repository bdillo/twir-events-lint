pub(super) mod config;

use std::{collections::HashSet, io::Read, time::Duration};

use anyhow::{Context, bail};
use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use reqwest::blocking::Client;
use serde::Deserialize;
use url::Url;

use crate::events::{CollectedEvent, CollectedEventsByRegion, Region};

use self::config::{EventFormat, LumaCalendar, validate_id};
use super::location::Location;

const API_URL: &str = "https://api.lu.ma/calendar/get-items";
const MAX_PAGE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_PAGES_PER_CALENDAR: usize = 100;
const PAGE_SIZE: &str = "50";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct LumaCollection {
    pub events: CollectedEventsByRegion,
    pub warnings: Vec<String>,
}

struct NormalizedEvent {
    event: CollectedEvent,
    regions: Vec<Region>,
}

#[derive(Debug, Deserialize)]
struct ApiPage {
    entries: Vec<ApiEntry>,
    has_more: bool,
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "platform")]
enum ApiEntry {
    #[serde(rename = "luma")]
    Luma {
        api_id: String,
        calendar_api_id: String,
        status: EntryStatus,
        event: Box<ApiEvent>,
        calendar: ApiCalendar,
    },
    #[serde(rename = "external")]
    External,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum EntryStatus {
    Approved,
}

#[derive(Debug, Deserialize)]
struct ApiEvent {
    api_id: String,
    calendar_api_id: String,
    name: String,
    start_at: String,
    timezone: String,
    url: String,
    #[serde(rename = "visibility")]
    _visibility: EventVisibility,
    location_type: LocationType,
    geo_address_info: Option<GeoAddress>,
}

#[derive(Debug, Deserialize)]
struct ApiCalendar {
    api_id: String,
    name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum EventVisibility {
    Public,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum LocationType {
    Discord,
    Meet,
    Missing,
    Offline,
    Twitch,
    Twitter,
    Unknown,
    Youtube,
    Zoom,
}

#[derive(Debug, Deserialize)]
struct GeoAddress {
    mode: AddressMode,
    city: Option<String>,
    region: Option<String>,
    region_short: Option<String>,
    country: Option<String>,
    country_code: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum AddressMode {
    Obfuscated,
    Shown,
}

#[derive(Clone, Copy)]
enum Period {
    Future,
    Past,
}

impl Period {
    fn as_str(self) -> &'static str {
        match self {
            Self::Future => "future",
            Self::Past => "past",
        }
    }
}

pub(crate) fn collect(
    calendars: Vec<LumaCalendar>,
    range_start: NaiveDate,
    range_end: NaiveDate,
) -> anyhow::Result<LumaCollection> {
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("twir-events-lint/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to build Luma HTTP client")?;
    let mut normalized = Vec::new();
    let mut warnings = Vec::new();

    for calendar in calendars {
        for period in periods_for_range(range_start, range_end) {
            let (events, period_warnings) =
                collect_period(&client, &calendar, period, range_start, range_end)?;
            normalized.extend(events);
            warnings.extend(period_warnings);
        }
    }

    let events = group_events(normalized, &mut warnings);
    Ok(LumaCollection { events, warnings })
}

fn periods_for_range(range_start: NaiveDate, range_end: NaiveDate) -> Vec<Period> {
    let today = Utc::now().date_naive();
    let mut periods = Vec::with_capacity(2);
    if range_start <= today {
        periods.push(Period::Past);
    }
    if range_end >= today {
        periods.push(Period::Future);
    }
    periods
}

fn collect_period(
    client: &Client,
    configured: &LumaCalendar,
    period: Period,
    range_start: NaiveDate,
    range_end: NaiveDate,
) -> anyhow::Result<(Vec<NormalizedEvent>, Vec<String>)> {
    let mut cursor = None;
    let mut seen_cursors = HashSet::new();
    let mut normalized = Vec::new();
    let mut warnings = Vec::new();

    for page_number in 1..=MAX_PAGES_PER_CALENDAR {
        let page = fetch_page(client, configured, period, cursor.as_deref())?;
        normalize_page(
            page.entries,
            configured,
            range_start,
            range_end,
            &mut normalized,
            &mut warnings,
        )?;
        cursor = validate_pagination(page.has_more, page.next_cursor).with_context(|| {
            format!(
                "invalid Luma pagination response for '{}'",
                configured.calendar_url
            )
        })?;
        let Some(next_cursor) = cursor.as_ref() else {
            return Ok((normalized, warnings));
        };
        if !seen_cursors.insert(next_cursor.clone()) {
            bail!(
                "Luma pagination repeated a cursor for '{}'",
                configured.calendar_url
            );
        }
        if page_number == MAX_PAGES_PER_CALENDAR {
            bail!(
                "Luma calendar '{}' exceeded the {MAX_PAGES_PER_CALENDAR}-page limit",
                configured.calendar_url
            );
        }
    }
    unreachable!("page loop either returns or fails at its limit")
}

fn fetch_page(
    client: &Client,
    configured: &LumaCalendar,
    period: Period,
    cursor: Option<&str>,
) -> anyhow::Result<ApiPage> {
    let mut request = client.get(API_URL).query(&[
        ("calendar_api_id", configured.calendar_id.as_str()),
        ("period", period.as_str()),
        ("pagination_limit", PAGE_SIZE),
    ]);
    if let Some(cursor) = cursor {
        request = request.query(&[("pagination_cursor", cursor)]);
    }
    let mut response = request
        .send()
        .with_context(|| {
            format!(
                "failed to fetch Luma calendar '{}'",
                configured.calendar_url
            )
        })?
        .error_for_status()
        .with_context(|| format!("Luma request failed for '{}'", configured.calendar_url))?;
    let mut contents = Vec::new();
    response
        .by_ref()
        .take(MAX_PAGE_BYTES + 1)
        .read_to_end(&mut contents)
        .with_context(|| format!("failed to read Luma calendar '{}'", configured.calendar_url))?;
    if contents.len() as u64 > MAX_PAGE_BYTES {
        bail!(
            "Luma calendar '{}' exceeded the 5 MiB page limit",
            configured.calendar_url
        );
    }
    serde_json::from_slice(&contents).with_context(|| {
        format!(
            "failed to parse Luma API response for '{}'",
            configured.calendar_url
        )
    })
}

fn validate_pagination(
    has_more: bool,
    next_cursor: Option<String>,
) -> anyhow::Result<Option<String>> {
    match (has_more, next_cursor) {
        (false, None) => Ok(None),
        (true, Some(cursor)) if !cursor.trim().is_empty() => Ok(Some(cursor)),
        (true, _) => bail!("has_more was true without a non-empty next_cursor"),
        (false, Some(_)) => bail!("has_more was false with a next_cursor"),
    }
}

fn normalize_page(
    entries: Vec<ApiEntry>,
    configured: &LumaCalendar,
    range_start: NaiveDate,
    range_end: NaiveDate,
    normalized: &mut Vec<NormalizedEvent>,
    warnings: &mut Vec<String>,
) -> anyhow::Result<()> {
    for entry in entries {
        let ApiEntry::Luma {
            api_id,
            calendar_api_id,
            status: EntryStatus::Approved,
            event,
            calendar,
        } = entry
        else {
            continue;
        };
        validate_id(&api_id, "calev-").context("invalid Luma calendar-entry ID")?;
        if calendar_api_id != configured.calendar_id {
            bail!(
                "Luma calendar response returned entry '{}' for unexpected calendar '{}'",
                api_id,
                calendar_api_id
            );
        }
        validate_id(&event.api_id, "evt-").context("invalid Luma event ID")?;
        validate_id(&event.calendar_api_id, "cal-").context("invalid event calendar ID")?;
        validate_id(&calendar.api_id, "cal-").context("invalid event calendar metadata ID")?;
        if calendar.api_id != event.calendar_api_id {
            bail!(
                "Luma event '{}' disagreed with its calendar metadata",
                event.api_id
            );
        }
        if event.calendar_api_id != configured.calendar_id {
            continue;
        }

        let date =
            event_date(&event).with_context(|| format!("invalid Luma event '{}'", event.api_id))?;
        if date < range_start || date > range_end {
            continue;
        }
        match normalize_event(*event, calendar, configured, date) {
            Ok(event) => normalized.push(event),
            Err(error) => warnings.push(format!(
                "skipping event from '{}': {error}",
                configured.calendar_url
            )),
        }
    }
    Ok(())
}

fn event_date(event: &ApiEvent) -> anyhow::Result<NaiveDate> {
    let start = DateTime::parse_from_rfc3339(&event.start_at)
        .with_context(|| format!("event '{}' had invalid start_at", event.api_id))?;
    let timezone = event
        .timezone
        .parse::<Tz>()
        .with_context(|| format!("event '{}' had invalid timezone", event.api_id))?;
    Ok(start.with_timezone(&timezone).date_naive())
}

fn normalize_event(
    event: ApiEvent,
    calendar: ApiCalendar,
    configured: &LumaCalendar,
    date: NaiveDate,
) -> anyhow::Result<NormalizedEvent> {
    let name = required_text(&event.name, "event name")?;
    let organizer_name = required_text(&calendar.name, "calendar name")?;
    let event_url = event_url(&event.url)?;
    let physical_location = if event.location_type == LocationType::Offline
        || configured.event_format == Some(EventFormat::Hybrid)
    {
        Some(
            event_location(event.geo_address_info.as_ref()).with_context(|| {
                format!(
                    "event '{}' omitted a usable physical location",
                    event.api_id
                )
            })?,
        )
    } else {
        None
    };
    let is_hybrid = configured.event_format == Some(EventFormat::Hybrid);
    let is_virtual = configured.event_format == Some(EventFormat::Virtual)
        || event.location_type != LocationType::Offline;
    let (location, regions) = if is_hybrid {
        let (location, region) = physical_location.expect("hybrid location was required");
        (location, vec![Region::Virtual, region])
    } else if is_virtual {
        (String::new(), vec![Region::Virtual])
    } else {
        let (location, region) = physical_location.expect("offline location was required");
        (location, vec![region])
    };

    Ok(NormalizedEvent {
        event: CollectedEvent {
            name,
            location,
            date: date.to_string(),
            url: event_url.to_string(),
            is_virtual,
            organizer_name,
            organizer_url: configured.calendar_url.to_string(),
            is_hybrid,
        },
        regions,
    })
}

fn required_text(value: &str, field: &str) -> anyhow::Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{field} was empty");
    }
    Ok(value.to_owned())
}

fn event_url(slug: &str) -> anyhow::Result<Url> {
    if slug.is_empty()
        || !slug
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        bail!("invalid Luma event URL slug '{slug}'");
    }
    Url::parse(&format!("https://luma.com/{slug}")).context("failed to construct Luma event URL")
}

fn event_location(address: Option<&GeoAddress>) -> anyhow::Result<(String, Region)> {
    let address = address.ok_or_else(|| anyhow::anyhow!("geo_address_info was null"))?;
    let _mode = address.mode;
    let state = address
        .region_short
        .clone()
        .or_else(|| address.region.clone());
    let country = address
        .country_code
        .clone()
        .or_else(|| country_code(address.country.as_deref()));
    let location = Location::new(address.city.clone(), state, country);
    let region = location
        .region()
        .ok_or_else(|| anyhow::anyhow!("location had an unknown country"))?;
    let display_name = location.display_name();
    if display_name.is_empty() {
        bail!("location had no displayable fields");
    }
    Ok((display_name, region))
}

fn country_code(country: Option<&str>) -> Option<String> {
    match country?.trim().to_lowercase().as_str() {
        "australia" => Some("AU".to_owned()),
        "brazil" | "brasil" => Some("BR".to_owned()),
        "mexico" | "méxico" => Some("MX".to_owned()),
        "spain" | "españa" => Some("ES".to_owned()),
        "türkiye" | "turkey" => Some("TR".to_owned()),
        "united kingdom" => Some("GB".to_owned()),
        "united states" | "united states of america" => Some("US".to_owned()),
        _ => None,
    }
}

fn group_events(
    normalized: Vec<NormalizedEvent>,
    warnings: &mut Vec<String>,
) -> CollectedEventsByRegion {
    let mut seen_urls = HashSet::new();
    let mut events = CollectedEventsByRegion::new();

    for normalized in normalized {
        let canonical_url = normalized.event.url.trim_end_matches('/');
        if !seen_urls.insert(canonical_url.to_owned()) {
            warnings.push(format!(
                "dropping duplicate Luma event URL '{}'",
                normalized.event.url
            ));
            continue;
        }
        for region in normalized.regions {
            events.add(normalized.event.clone(), region);
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_calendar(event_format: Option<EventFormat>) -> LumaCalendar {
        LumaCalendar {
            calendar_url: Url::parse("https://luma.com/rust-test").unwrap(),
            calendar_id: "cal-TestCalendar123".to_owned(),
            event_format,
        }
    }

    fn fixture_page() -> ApiPage {
        serde_json::from_str(include_str!("../../../tests/fixtures/luma-api.json")).unwrap()
    }

    #[test]
    fn parses_and_normalizes_api_events() {
        let page = fixture_page();
        let start = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let mut normalized = Vec::new();
        let mut warnings = Vec::new();

        normalize_page(
            page.entries,
            &configured_calendar(None),
            start,
            end,
            &mut normalized,
            &mut warnings,
        )
        .unwrap();

        assert!(warnings.is_empty());
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].event.name, "Physical Rust Event");
        assert_eq!(normalized[0].event.date, "2026-10-20");
        assert_eq!(normalized[0].event.location, "New York, NY, US");
        assert_eq!(normalized[0].regions, vec![Region::NorthAmerica]);
        assert_eq!(normalized[1].event.name, "Virtual Rust Event");
        assert_eq!(normalized[1].event.location, "");
        assert_eq!(normalized[1].regions, vec![Region::Virtual]);
    }

    #[test]
    fn configured_hybrid_events_use_physical_and_virtual_regions() {
        let page = fixture_page();
        let date = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let mut normalized = Vec::new();
        let mut warnings = Vec::new();

        normalize_page(
            page.entries,
            &configured_calendar(Some(EventFormat::Hybrid)),
            date,
            date,
            &mut normalized,
            &mut warnings,
        )
        .unwrap();

        assert!(warnings.is_empty());
        assert_eq!(normalized.len(), 1);
        assert!(normalized[0].event.is_hybrid);
        assert_eq!(
            normalized[0].regions,
            vec![Region::Virtual, Region::NorthAmerica]
        );
    }

    #[test]
    fn excludes_cross_listed_events() {
        let page = fixture_page();
        let mut normalized = Vec::new();
        let mut warnings = Vec::new();

        normalize_page(
            page.entries,
            &configured_calendar(None),
            NaiveDate::MIN,
            NaiveDate::MAX,
            &mut normalized,
            &mut warnings,
        )
        .unwrap();

        assert_eq!(normalized.len(), 2);
        assert!(
            normalized
                .iter()
                .all(|event| event.event.name != "Cross-listed Rust Event")
        );
    }

    #[test]
    fn rejects_inconsistent_pagination() {
        assert!(validate_pagination(true, None).is_err());
        assert!(validate_pagination(false, Some("cursor".to_owned())).is_err());
        assert_eq!(
            validate_pagination(true, Some("cursor".to_owned())).unwrap(),
            Some("cursor".to_owned())
        );
    }

    #[test]
    fn rejects_missing_required_event_fields() {
        let fixture = include_str!("../../../tests/fixtures/luma-api.json")
            .replace("\"timezone\": \"America/New_York\",", "");

        assert!(serde_json::from_str::<ApiPage>(&fixture).is_err());
    }

    #[test]
    fn rejects_unknown_location_types() {
        let fixture = include_str!("../../../tests/fixtures/luma-api.json")
            .replace("\"offline\"", "\"teleport\"");

        assert!(serde_json::from_str::<ApiPage>(&fixture).is_err());
    }
}
