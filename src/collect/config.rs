use std::{fs, path::Path};

use anyhow::{Context, bail};
use serde::Deserialize;

use super::{
    luma::config::{ConfiguredCalendar, LumaCalendar, validate_calendars},
    meetup::config::{ConfiguredGroup, MeetupGroup, validate_groups},
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfiguredSources {
    meetup: Vec<ConfiguredGroup>,
    luma: Vec<ConfiguredCalendar>,
}

pub struct EventSources {
    pub meetup: Vec<MeetupGroup>,
    pub luma: Vec<LumaCalendar>,
}

pub fn read_sources(path: &Path) -> anyhow::Result<EventSources> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read event sources from {}", path.display()))?;
    let configured: ConfiguredSources = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse event sources from {}", path.display()))?;
    validate_sources(configured)
        .with_context(|| format!("invalid event sources in {}", path.display()))
}

fn validate_sources(configured: ConfiguredSources) -> anyhow::Result<EventSources> {
    if configured.meetup.is_empty() && configured.luma.is_empty() {
        bail!("no event sources configured");
    }
    let meetup = validate_groups(configured.meetup).context("invalid Meetup configuration")?;
    let luma = validate_calendars(configured.luma).context("invalid Luma configuration")?;
    Ok(EventSources { meetup, luma })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_source_configuration() {
        let configured: ConfiguredSources =
            serde_json::from_str(r#"{"meetup":[],"luma":[]}"#).unwrap();

        assert!(validate_sources(configured).is_err());
    }
}
