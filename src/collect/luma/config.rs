use std::collections::HashSet;

use anyhow::{Context, bail};
use chrono_tz::Tz;
use serde::Deserialize;
use url::Url;

use crate::events::Region;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventFormat {
    Virtual,
    Hybrid,
}

#[derive(Clone, Debug)]
pub struct LumaCalendar {
    pub calendar_url: Url,
    pub ical_url: Url,
    pub default_location: String,
    pub region: Region,
    pub timezone: Tz,
    pub event_format: Option<EventFormat>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfiguredCalendar {
    calendar_url: String,
    ical_url: String,
    default_location: String,
    region: Region,
    timezone: String,
    event_format: Option<EventFormat>,
}

pub fn validate_calendars(
    configured: Vec<ConfiguredCalendar>,
) -> anyhow::Result<Vec<LumaCalendar>> {
    let mut feed_urls = HashSet::new();
    let mut calendars = Vec::with_capacity(configured.len());
    for calendar in configured {
        let calendar = validate_calendar(calendar)?;
        if !feed_urls.insert(calendar.ical_url.as_str().to_owned()) {
            bail!("duplicate Luma iCalendar URL '{}'", calendar.ical_url);
        }
        calendars.push(calendar);
    }
    calendars.sort_by(|left, right| left.calendar_url.as_str().cmp(right.calendar_url.as_str()));
    Ok(calendars)
}

fn validate_calendar(configured: ConfiguredCalendar) -> anyhow::Result<LumaCalendar> {
    let calendar_url = Url::parse(&configured.calendar_url)
        .with_context(|| format!("invalid Luma calendar URL '{}'", configured.calendar_url))?;
    if calendar_url.scheme() != "https" || calendar_url.host_str() != Some("luma.com") {
        bail!(
            "invalid Luma calendar URL '{}', expected an https://luma.com URL",
            configured.calendar_url
        );
    }

    let ical_url = Url::parse(&configured.ical_url)
        .with_context(|| format!("invalid Luma iCalendar URL '{}'", configured.ical_url))?;
    let is_calendar_feed = ical_url
        .query_pairs()
        .any(|(key, value)| key == "entity" && value == "calendar");
    let has_calendar_id = ical_url
        .query_pairs()
        .any(|(key, value)| key == "id" && value.starts_with("cal-"));
    if ical_url.scheme() != "https"
        || ical_url.host_str() != Some("api.lu.ma")
        || ical_url.path() != "/ics/get"
        || !is_calendar_feed
        || !has_calendar_id
    {
        bail!(
            "invalid Luma iCalendar URL '{}', expected a public Luma calendar feed",
            configured.ical_url
        );
    }

    let default_location = configured.default_location.trim().to_owned();
    if default_location.is_empty() {
        bail!(
            "default location is empty for Luma calendar '{}'",
            configured.calendar_url
        );
    }
    let timezone = configured.timezone.parse::<Tz>().with_context(|| {
        format!(
            "invalid timezone '{}' for Luma calendar '{}'",
            configured.timezone, configured.calendar_url
        )
    })?;

    Ok(LumaCalendar {
        calendar_url,
        ical_url,
        default_location,
        region: configured.region,
        timezone,
        event_format: configured.event_format,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_calendar() -> ConfiguredCalendar {
        ConfiguredCalendar {
            calendar_url: "https://luma.com/rust-girona".to_owned(),
            ical_url: "https://api.lu.ma/ics/get?entity=calendar&id=cal-YjQVtnwkdU40fBI".to_owned(),
            default_location: "Girona, ES".to_owned(),
            region: Region::Europe,
            timezone: "Europe/Madrid".to_owned(),
            event_format: None,
        }
    }

    #[test]
    fn validates_calendar_configuration() {
        let calendar = validate_calendar(configured_calendar()).unwrap();

        assert_eq!(calendar.default_location, "Girona, ES");
        assert_eq!(calendar.region, Region::Europe);
        assert_eq!(calendar.timezone, chrono_tz::Europe::Madrid);
    }

    #[test]
    fn rejects_non_luma_feed_urls() {
        let mut configured = configured_calendar();
        configured.ical_url = "https://example.com/calendar.ics".to_owned();

        assert!(validate_calendar(configured).is_err());
    }
}
