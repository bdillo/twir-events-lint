use std::collections::HashSet;

use anyhow::{Context, bail};
use serde::Deserialize;
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventFormat {
    Virtual,
    Hybrid,
}

#[derive(Clone, Debug)]
pub struct LumaCalendar {
    pub calendar_url: Url,
    pub calendar_id: String,
    pub event_format: Option<EventFormat>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfiguredCalendar {
    calendar_url: String,
    calendar_id: String,
    event_format: Option<EventFormat>,
}

pub fn validate_calendars(
    configured: Vec<ConfiguredCalendar>,
) -> anyhow::Result<Vec<LumaCalendar>> {
    let mut calendar_ids = HashSet::new();
    let mut calendar_urls = HashSet::new();
    let mut calendars = Vec::with_capacity(configured.len());
    for calendar in configured {
        let calendar = validate_calendar(calendar)?;
        if !calendar_ids.insert(calendar.calendar_id.clone()) {
            bail!("duplicate Luma calendar ID '{}'", calendar.calendar_id);
        }
        if !calendar_urls.insert(calendar.calendar_url.as_str().to_owned()) {
            bail!("duplicate Luma calendar URL '{}'", calendar.calendar_url);
        }
        calendars.push(calendar);
    }
    calendars.sort_by(|left, right| left.calendar_url.as_str().cmp(right.calendar_url.as_str()));
    Ok(calendars)
}

fn validate_calendar(configured: ConfiguredCalendar) -> anyhow::Result<LumaCalendar> {
    let calendar_url = Url::parse(&configured.calendar_url)
        .with_context(|| format!("invalid Luma calendar URL '{}'", configured.calendar_url))?;
    if calendar_url.scheme() != "https"
        || calendar_url.host_str() != Some("luma.com")
        || calendar_url.query().is_some()
        || calendar_url.fragment().is_some()
    {
        bail!(
            "invalid Luma calendar URL '{}', expected a canonical https://luma.com URL",
            configured.calendar_url
        );
    }
    validate_id(&configured.calendar_id, "cal-")
        .with_context(|| format!("invalid Luma calendar ID '{}'", configured.calendar_id))?;

    Ok(LumaCalendar {
        calendar_url,
        calendar_id: configured.calendar_id,
        event_format: configured.event_format,
    })
}

pub(super) fn validate_id(value: &str, prefix: &str) -> anyhow::Result<()> {
    let suffix = value
        .strip_prefix(prefix)
        .filter(|suffix| !suffix.is_empty())
        .ok_or_else(|| anyhow::anyhow!("expected prefix '{prefix}'"))?;
    if !suffix
        .chars()
        .all(|character| character.is_ascii_alphanumeric())
    {
        bail!("expected only ASCII letters and digits after '{prefix}'");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_calendar() -> ConfiguredCalendar {
        ConfiguredCalendar {
            calendar_url: "https://luma.com/rust-girona".to_owned(),
            calendar_id: "cal-YjQVtnwkdU40fBI".to_owned(),
            event_format: None,
        }
    }

    #[test]
    fn validates_calendar_configuration() {
        let calendar = validate_calendar(configured_calendar()).unwrap();

        assert_eq!(calendar.calendar_id, "cal-YjQVtnwkdU40fBI");
        assert_eq!(
            calendar.calendar_url.as_str(),
            "https://luma.com/rust-girona"
        );
    }

    #[test]
    fn rejects_invalid_calendar_ids() {
        let mut configured = configured_calendar();
        configured.calendar_id = "calendar-123".to_owned();

        assert!(validate_calendar(configured).is_err());
    }
}
