use std::{collections::HashSet, fs, path::Path};

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
pub struct MeetupGroup {
    pub url: Url,
    pub url_name: String,
    pub event_format: Option<EventFormat>,
    pub required_title_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfiguredGroup {
    url: String,
    event_format: Option<EventFormat>,
    required_title_token: Option<String>,
}

pub fn read_groups(path: &Path) -> anyhow::Result<Vec<MeetupGroup>> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read Meetup groups from {}", path.display()))?;
    let configured: Vec<ConfiguredGroup> = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse Meetup groups from {}", path.display()))?;

    let mut names = HashSet::new();
    let mut groups = Vec::with_capacity(configured.len());
    for configured_group in configured {
        let group = parse_group(
            &configured_group.url,
            configured_group.event_format,
            configured_group.required_title_token,
        )?;
        if !names.insert(group.url_name.clone()) {
            bail!("duplicate Meetup group name '{}'", group.url_name);
        }
        groups.push(group);
    }
    groups.sort_by(|left, right| left.url.as_str().cmp(right.url.as_str()));
    Ok(groups)
}

fn parse_group(
    url: &str,
    event_format: Option<EventFormat>,
    required_title_token: Option<String>,
) -> anyhow::Result<MeetupGroup> {
    let parsed = Url::parse(url).with_context(|| format!("invalid Meetup group URL '{url}'"))?;
    if parsed.host_str() != Some("www.meetup.com") {
        bail!("invalid Meetup group host in '{url}', expected www.meetup.com");
    }
    let required_title_token = match required_title_token {
        Some(token) => {
            let token = token.trim().to_lowercase();
            if token.is_empty()
                || !token
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
            {
                bail!("invalid required title token for Meetup group '{url}'");
            }
            Some(token)
        }
        None => None,
    };

    let url_name = parsed
        .path_segments()
        .and_then(|mut segments| segments.find(|segment| !segment.is_empty()))
        .filter(|segment| *segment != "events")
        .ok_or_else(|| anyhow::anyhow!("unable to find Meetup group name in '{url}'"))?
        .to_owned();

    Ok(MeetupGroup {
        url: parsed,
        url_name,
        event_format,
        required_title_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_group_name_and_event_format() {
        let group = parse_group(
            "https://www.meetup.com/rust-noris/events/",
            Some(EventFormat::Virtual),
            None,
        )
        .unwrap();

        assert_eq!(group.url_name, "rust-noris");
        assert_eq!(group.event_format, Some(EventFormat::Virtual));
    }

    #[test]
    fn rejects_non_meetup_hosts() {
        assert!(parse_group("https://example.com/rust", None, None).is_err());
    }

    #[test]
    fn reads_existing_group_configuration() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("groups/rust-meetups.json");

        let groups = read_groups(&path).unwrap();

        assert_eq!(groups.len(), 142);
        assert!(groups.iter().any(|group| {
            group.url_name == "vancouver-rust" && group.event_format == Some(EventFormat::Hybrid)
        }));
        assert_eq!(
            groups
                .iter()
                .filter(|group| group.required_title_token.is_some())
                .count(),
            26
        );
    }

    #[test]
    fn rejects_invalid_required_title_tokens() {
        assert!(
            parse_group(
                "https://www.meetup.com/test-rust",
                None,
                Some("rust lang".to_owned())
            )
            .is_err()
        );
    }
}
