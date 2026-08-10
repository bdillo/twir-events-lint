mod api;
mod auth;
mod config;
mod location;

use std::{collections::HashSet, path::Path};

use anyhow::{Context, bail};
use chrono::NaiveDate;
use url::Url;

use crate::events::{CollectedEvent, CollectedEventsByRegion, Region};

use self::{
    api::{ApiEvent, ApiGroup, ApiVenue, MeetupClient},
    auth::MeetupCredentials,
    config::{EventFormat, MeetupGroup, read_groups},
    location::Location,
};

const MAX_PAGES_PER_GROUP: usize = 100;

pub struct MeetupCollection {
    pub events: CollectedEventsByRegion,
    pub warnings: Vec<String>,
}

struct NormalizedEvent {
    event: CollectedEvent,
    date: NaiveDate,
    location: String,
    regions: Vec<Region>,
}

pub fn collect(
    groups_path: &Path,
    range_start: NaiveDate,
    range_end: NaiveDate,
) -> anyhow::Result<MeetupCollection> {
    let groups = read_groups(groups_path)?;
    let credentials = MeetupCredentials::from_env()?;
    let client = MeetupClient::new()?;
    let token = client.authenticate(&credentials)?;
    let mut raw_events = Vec::new();
    let mut warnings = Vec::new();

    for group in groups {
        let mut cursor = None;
        let mut seen_cursors = HashSet::new();

        for _ in 0..MAX_PAGES_PER_GROUP {
            let page = client
                .event_page(
                    &token,
                    &group.url_name,
                    cursor.as_deref(),
                    range_start,
                    range_end,
                )
                .with_context(|| format!("failed to collect Meetup group '{}'", group.url))?;
            let Some(page) = page else {
                warnings.push(format!("meetup group '{}' was not found", group.url));
                break;
            };
            let reached_range_end = page
                .events
                .iter()
                .filter_map(|event| event.date_time.as_deref())
                .filter_map(|date_time| parse_event_date(date_time).ok())
                .any(|date| date > range_end);
            raw_events.extend(page.events.into_iter().map(|event| (group.clone(), event)));
            if reached_range_end {
                break;
            }

            let Some(next_cursor) = page.next_cursor else {
                break;
            };
            if !seen_cursors.insert(next_cursor.clone()) {
                bail!(
                    "meetup pagination repeated cursor for group '{}'",
                    group.url
                );
            }
            cursor = Some(next_cursor);
        }

        if cursor.is_some() && seen_cursors.len() == MAX_PAGES_PER_GROUP {
            bail!("meetup group '{}' exceeded pagination limit", group.url);
        }
    }

    let mut normalized = Vec::new();
    for (group, event) in raw_events {
        match normalize_event(event, &group) {
            Ok(event) if !date_in_range(event.date, range_start, range_end) => {
                warnings.push(format!(
                    "dropping out-of-range meetup event '{}' on {}",
                    event.event.name, event.date
                ));
            }
            Ok(event) => normalized.push(event),
            Err(error) => warnings.push(format!("skipping event from '{}': {error}", group.url)),
        }
    }
    let events = group_events(normalized, &mut warnings);
    Ok(MeetupCollection { events, warnings })
}

fn date_in_range(date: NaiveDate, start: NaiveDate, end: NaiveDate) -> bool {
    date >= start && date <= end
}

fn group_events(
    mut normalized: Vec<NormalizedEvent>,
    warnings: &mut Vec<String>,
) -> CollectedEventsByRegion {
    normalized.sort_by(|left, right| {
        (&left.date, &left.location, &left.event.url).cmp(&(
            &right.date,
            &right.location,
            &right.event.url,
        ))
    });

    let mut seen_urls = HashSet::new();
    let mut events = CollectedEventsByRegion::new();
    for normalized in normalized {
        let canonical_url = normalized.event.url.trim_end_matches('/');
        if !seen_urls.insert(canonical_url.to_owned()) {
            warnings.push(format!(
                "dropping duplicate meetup event URL '{}'",
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

fn normalize_event(
    event: ApiEvent,
    configured_group: &MeetupGroup,
) -> anyhow::Result<NormalizedEvent> {
    let ApiEvent {
        group,
        title,
        date_time,
        event_url,
        venues,
    } = event;
    let group = group.ok_or_else(|| anyhow::anyhow!("response omitted the group"))?;
    let title = required_text(title, "title")?;
    let date_time = required_text(date_time, "dateTime")?;
    let event_url = required_text(event_url, "eventUrl")?;
    Url::parse(&event_url).context("invalid event URL")?;
    let date = parse_event_date(&date_time)?;

    let group_location = group_location(&group);
    let venue = venue(venues, configured_group.event_format, &group)?;
    let event_location = Location::new(venue.city, venue.state, venue.country);
    let location = if event_location.fields_present() > group_location.fields_present() {
        event_location
    } else {
        group_location
    };
    let location_name = location.display_name();
    if location_name.is_empty() {
        bail!("event and group locations are empty");
    }

    let venue_is_virtual = venue.venue_type.as_deref() == Some("online");
    let (is_virtual, is_hybrid) = match configured_group.event_format {
        Some(EventFormat::Virtual) => (true, false),
        Some(EventFormat::Hybrid) => (venue_is_virtual, true),
        None => (venue_is_virtual, false),
    };
    let regions = if is_hybrid {
        vec![
            Region::Virtual,
            location
                .region()
                .ok_or_else(|| anyhow::anyhow!("unknown or missing country code"))?,
        ]
    } else if is_virtual {
        vec![Region::Virtual]
    } else {
        vec![
            location
                .region()
                .ok_or_else(|| anyhow::anyhow!("unknown or missing country code"))?,
        ]
    };
    let organizer_name = required_text(group.name, "group name")?;

    Ok(NormalizedEvent {
        event: CollectedEvent {
            name: title,
            location: location_name.clone(),
            date: date.format("%Y-%m-%d").to_string(),
            url: event_url,
            is_virtual,
            organizer_name,
            organizer_url: configured_group.url.to_string(),
            is_hybrid,
        },
        date,
        location: location_name,
        regions,
    })
}

fn group_location(group: &ApiGroup) -> Location {
    Location::new(
        group.city.clone(),
        group.state.clone(),
        group.country.clone(),
    )
}

fn venue(
    mut venues: Vec<ApiVenue>,
    event_format: Option<EventFormat>,
    group: &ApiGroup,
) -> anyhow::Result<ApiVenue> {
    if !venues.is_empty() {
        return Ok(venues.remove(0));
    }
    if event_format == Some(EventFormat::Virtual) {
        return Ok(ApiVenue {
            city: group.city.clone(),
            state: group.state.clone(),
            country: group.country.clone(),
            venue_type: Some("online".to_owned()),
        });
    }
    bail!("response omitted venue information")
}

fn parse_event_date(date_time: &str) -> anyhow::Result<NaiveDate> {
    let date = date_time
        .split_once('T')
        .map_or(date_time, |(date, _)| date);
    NaiveDate::parse_from_str(date, "%Y-%m-%d").context("invalid event dateTime")
}

fn required_text(value: Option<String>, field: &str) -> anyhow::Result<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("response omitted {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(event_format: Option<EventFormat>) -> MeetupGroup {
        MeetupGroup {
            url: Url::parse("https://www.meetup.com/test-rust").unwrap(),
            url_name: "test-rust".to_owned(),
            event_format,
        }
    }

    fn api_event(venue_type: &str) -> ApiEvent {
        ApiEvent {
            group: Some(ApiGroup {
                name: Some(" Test Rust ".to_owned()),
                city: Some("London".to_owned()),
                state: Some("SW1".to_owned()),
                country: Some("GB".to_owned()),
            }),
            title: Some(" Test Event ".to_owned()),
            date_time: Some("2026-05-14T19:00+01:00".to_owned()),
            event_url: Some("https://www.meetup.com/test-rust/events/1/".to_owned()),
            venues: vec![ApiVenue {
                city: Some("London".to_owned()),
                state: Some("SW1".to_owned()),
                country: Some("GB".to_owned()),
                venue_type: Some(venue_type.to_owned()),
            }],
        }
    }

    #[test]
    fn normalizes_virtual_meetup_event() {
        let event = normalize_event(api_event("online"), &group(None)).unwrap();

        assert_eq!(event.event.name, "Test Event");
        assert_eq!(event.event.location, "London, UK");
        assert!(event.event.is_virtual);
        assert_eq!(event.regions, vec![Region::Virtual]);
    }

    #[test]
    fn hybrid_event_is_added_to_virtual_and_geographic_regions() {
        let event =
            normalize_event(api_event("physical"), &group(Some(EventFormat::Hybrid))).unwrap();

        assert!(event.event.is_hybrid);
        assert_eq!(event.regions, vec![Region::Virtual, Region::Europe]);
    }

    #[test]
    fn virtual_event_format_supplies_missing_venue() {
        let mut event = api_event("online");
        event.venues.clear();
        let event = normalize_event(event, &group(Some(EventFormat::Virtual))).unwrap();

        assert!(event.event.is_virtual);
        assert_eq!(event.event.location, "London, UK");
    }

    #[test]
    fn non_us_state_still_helps_select_the_event_venue() {
        let mut event = api_event("physical");
        event.venues[0] = ApiVenue {
            city: Some("Montreal".to_owned()),
            state: Some("QC".to_owned()),
            country: Some("CA".to_owned()),
            venue_type: Some("physical".to_owned()),
        };

        let event = normalize_event(event, &group(None)).unwrap();

        assert_eq!(event.event.location, "Montreal, CA");
        assert_eq!(event.regions, vec![Region::NorthAmerica]);
    }

    #[test]
    fn unknown_country_is_rejected_for_physical_event() {
        let mut event = api_event("physical");
        event.venues[0].country = Some("ZZ".to_owned());
        event.group.as_mut().unwrap().country = Some("ZZ".to_owned());

        assert!(normalize_event(event, &group(None)).is_err());
    }

    #[test]
    fn date_range_includes_both_boundaries() {
        let start = NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 6, 11).unwrap();

        assert!(date_in_range(start, start, end));
        assert!(date_in_range(end, start, end));
        assert!(!date_in_range(start.pred_opt().unwrap(), start, end));
        assert!(!date_in_range(end.succ_opt().unwrap(), start, end));
    }

    #[test]
    fn duplicate_urls_are_removed_before_region_grouping() {
        let first = normalize_event(api_event("online"), &group(None)).unwrap();
        let second = normalize_event(api_event("online"), &group(None)).unwrap();
        let mut warnings = Vec::new();

        let grouped = group_events(vec![first, second], &mut warnings);
        let json = serde_json::to_value(grouped).unwrap();

        assert_eq!(json["Virtual"].as_array().unwrap().len(), 1);
        assert_eq!(warnings.len(), 1);
    }
}
