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
    use crate::collect::meetup::config::EventFormat;

    #[test]
    fn reads_repository_event_sources() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("groups/rust-event-sources.json");

        let sources = read_sources(&path).unwrap();

        assert_eq!(sources.meetup.len(), 142);
        assert!(sources.meetup.iter().any(|group| {
            group.url_name == "vancouver-rust" && group.event_format == Some(EventFormat::Hybrid)
        }));
        assert_eq!(
            sources
                .meetup
                .iter()
                .filter(|group| group.required_title_token.is_some())
                .count(),
            26
        );
        assert_eq!(sources.luma.len(), 1);
        assert_eq!(
            sources.luma[0].calendar_url.as_str(),
            "https://luma.com/rust-girona"
        );
    }

    #[test]
    fn rejects_empty_source_configuration() {
        let configured: ConfiguredSources =
            serde_json::from_str(r#"{"meetup":[],"luma":[]}"#).unwrap();

        assert!(validate_sources(configured).is_err());
    }
}
