pub(super) mod config;

use std::{collections::HashSet, io::Read, time::Duration};

use anyhow::{Context, bail};
use chrono::NaiveDate;
use icalendar::{Calendar, CalendarDateTime, Component, DatePerhapsTime, EventLike, EventStatus};
use reqwest::blocking::Client;
use url::Url;

use crate::events::{CollectedEvent, CollectedEventsByRegion, Region};

use self::config::{EventFormat, LumaCalendar};

const MAX_FEED_BYTES: u64 = 5 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct LumaCollection {
    pub events: CollectedEventsByRegion,
    pub warnings: Vec<String>,
}

struct NormalizedEvent {
    event: CollectedEvent,
    date: NaiveDate,
    regions: Vec<Region>,
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

    for configured in calendars {
        let contents = fetch_calendar(&client, &configured)?;
        normalized.extend(parse_calendar(
            &contents,
            &configured,
            range_start,
            range_end,
            &mut warnings,
        )?);
    }

    let events = group_events(normalized, &mut warnings);
    Ok(LumaCollection { events, warnings })
}

fn fetch_calendar(client: &Client, configured: &LumaCalendar) -> anyhow::Result<String> {
    let mut response = client
        .get(configured.ical_url.clone())
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
        .take(MAX_FEED_BYTES + 1)
        .read_to_end(&mut contents)
        .with_context(|| format!("failed to read Luma calendar '{}'", configured.calendar_url))?;
    if contents.len() as u64 > MAX_FEED_BYTES {
        bail!(
            "Luma calendar '{}' exceeded the 5 MiB response limit",
            configured.calendar_url
        );
    }
    String::from_utf8(contents)
        .with_context(|| format!("Luma calendar '{}' was not UTF-8", configured.calendar_url))
}

fn parse_calendar(
    contents: &str,
    configured: &LumaCalendar,
    range_start: NaiveDate,
    range_end: NaiveDate,
    warnings: &mut Vec<String>,
) -> anyhow::Result<Vec<NormalizedEvent>> {
    let calendar: Calendar = contents.parse().map_err(|error| {
        anyhow::anyhow!(
            "failed to parse Luma calendar '{}': {error}",
            configured.calendar_url
        )
    })?;
    let organizer_name = calendar
        .get_name()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Luma calendar '{}' omitted its name",
                configured.calendar_url
            )
        })?;
    let mut normalized = Vec::new();

    for event in calendar.events() {
        if event.get_status() == Some(EventStatus::Cancelled) {
            continue;
        }
        let event_name = event.get_summary().unwrap_or("unnamed event");
        if event.property_value("RRULE").is_some() {
            warnings.push(format!(
                "Luma event '{event_name}' contains an unsupported recurrence rule; including only its listed occurrence"
            ));
        }
        match normalize_event(event, configured, organizer_name) {
            Ok(event) if event_date_in_range(&event, range_start, range_end) => {
                normalized.push(event);
            }
            Ok(_) => {}
            Err(error) => warnings.push(format!(
                "skipping event from '{}': {error}",
                configured.calendar_url
            )),
        }
    }

    Ok(normalized)
}

fn normalize_event(
    event: &icalendar::Event,
    configured: &LumaCalendar,
    organizer_name: &str,
) -> anyhow::Result<NormalizedEvent> {
    let name = required_text(event.get_summary(), "summary")?;
    let date = event_date(
        event
            .get_start()
            .ok_or_else(|| anyhow::anyhow!("event '{name}' omitted DTSTART"))?,
        configured.timezone,
    );
    let event_url =
        event_url(event).with_context(|| format!("event '{name}' omitted its public Luma URL"))?;
    let location_is_virtual = event.get_location().is_some_and(is_virtual_location);
    let (is_virtual, is_hybrid) = match configured.event_format {
        Some(EventFormat::Virtual) => (true, false),
        Some(EventFormat::Hybrid) => (location_is_virtual, true),
        None => (location_is_virtual, false),
    };
    let regions = if is_hybrid {
        vec![Region::Virtual, configured.region]
    } else if is_virtual {
        vec![Region::Virtual]
    } else {
        vec![configured.region]
    };

    Ok(NormalizedEvent {
        event: CollectedEvent {
            name,
            location: configured.default_location.clone(),
            date: date.to_string(),
            url: event_url.to_string(),
            is_virtual,
            organizer_name: organizer_name.to_owned(),
            organizer_url: configured.calendar_url.to_string(),
            is_hybrid,
        },
        date,
        regions,
    })
}

fn required_text(value: Option<&str>, field: &str) -> anyhow::Result<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("event omitted {field}"))
}

fn event_date(start: DatePerhapsTime, timezone: chrono_tz::Tz) -> NaiveDate {
    match start {
        DatePerhapsTime::Date(date) => date,
        DatePerhapsTime::DateTime(CalendarDateTime::Utc(date_time)) => {
            date_time.with_timezone(&timezone).date_naive()
        }
        DatePerhapsTime::DateTime(CalendarDateTime::Floating(date_time))
        | DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone { date_time, tzid: _ }) => {
            date_time.date()
        }
    }
}

fn is_virtual_location(location: &str) -> bool {
    Url::parse(location.trim())
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn event_url(event: &icalendar::Event) -> anyhow::Result<Url> {
    event
        .get_url()
        .into_iter()
        .chain(
            event
                .get_description()
                .into_iter()
                .flat_map(str::split_whitespace),
        )
        .chain(event.get_location())
        .filter_map(parse_luma_url)
        .next()
        .ok_or_else(|| anyhow::anyhow!("no public Luma URL found"))
}

fn parse_luma_url(candidate: &str) -> Option<Url> {
    let candidate = candidate.trim_matches(|character: char| {
        matches!(character, '(' | ')' | '[' | ']' | '<' | '>' | ',' | ';')
    });
    let url = Url::parse(candidate).ok()?;
    matches!(url.host_str(), Some("luma.com" | "lu.ma")).then_some(url)
}

fn event_date_in_range(
    event: &NormalizedEvent,
    range_start: NaiveDate,
    range_end: NaiveDate,
) -> bool {
    event.date >= range_start && event.date <= range_end
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
            calendar_url: Url::parse("https://luma.com/rust-girona").unwrap(),
            ical_url: Url::parse(
                "https://api.lu.ma/ics/get?entity=calendar&id=cal-YjQVtnwkdU40fBI",
            )
            .unwrap(),
            default_location: "Girona, ES".to_owned(),
            region: Region::Europe,
            timezone: chrono_tz::Europe::Madrid,
            event_format,
        }
    }

    #[test]
    fn parses_luma_feed_and_normalizes_events() {
        let contents = include_str!("../../../tests/fixtures/luma.ics");
        let start = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let mut warnings = Vec::new();

        let normalized = parse_calendar(
            contents,
            &configured_calendar(None),
            start,
            end,
            &mut warnings,
        )
        .unwrap();

        assert_eq!(normalized.len(), 2);
        assert!(warnings.is_empty());
        assert_eq!(normalized[0].event.name, "Physical Rust Event");
        assert_eq!(normalized[0].event.date, "2026-10-21");
        assert_eq!(normalized[0].event.url, "https://luma.com/physical");
        assert!(!normalized[0].event.is_virtual);
        assert_eq!(normalized[0].regions, vec![Region::Europe]);
        assert_eq!(normalized[1].event.name, "Virtual Rust Event");
        assert!(normalized[1].event.is_virtual);
        assert_eq!(normalized[1].regions, vec![Region::Virtual]);
    }

    #[test]
    fn configured_hybrid_events_are_added_to_both_regions() {
        let contents = include_str!("../../../tests/fixtures/luma.ics");
        let start = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let mut warnings = Vec::new();

        let normalized = parse_calendar(
            contents,
            &configured_calendar(Some(EventFormat::Hybrid)),
            start,
            end,
            &mut warnings,
        )
        .unwrap();

        assert!(normalized.iter().all(|event| event.event.is_hybrid));
        assert!(
            normalized
                .iter()
                .all(|event| event.regions == vec![Region::Virtual, Region::Europe])
        );
    }

    #[test]
    fn rejects_non_luma_event_urls() {
        assert!(parse_luma_url("https://example.com/event").is_none());
    }

    #[test]
    fn distinguishes_virtual_and_physical_locations() {
        assert!(is_virtual_location("https://luma.com/event/evt-test"));
        assert!(!is_virtual_location(
            "Carrer dels Mercaders, 5, Girona, Spain"
        ));
    }
}
