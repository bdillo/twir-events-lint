use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::{Context, bail};
use serde::Deserialize;
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LocationOverride {
    Virtual,
    Hybrid,
}

#[derive(Clone, Debug)]
pub struct MeetupGroup {
    pub url: Url,
    pub url_name: String,
    pub location_override: Option<LocationOverride>,
}

#[derive(Debug, Default, Deserialize)]
struct GroupMetadata {
    location_override: Option<LocationOverride>,
}

pub fn read_groups(path: &Path) -> anyhow::Result<Vec<MeetupGroup>> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read Meetup groups from {}", path.display()))?;
    let configured: HashMap<String, GroupMetadata> = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse Meetup groups from {}", path.display()))?;

    let mut names = HashSet::new();
    let mut groups = Vec::with_capacity(configured.len());
    for (url, metadata) in configured {
        let group = parse_group(&url, metadata.location_override)?;
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
    location_override: Option<LocationOverride>,
) -> anyhow::Result<MeetupGroup> {
    let parsed = Url::parse(url).with_context(|| format!("invalid Meetup group URL '{url}'"))?;
    if parsed.host_str() != Some("www.meetup.com") {
        bail!("invalid Meetup group host in '{url}', expected www.meetup.com");
    }
    let url_name = parsed
        .path_segments()
        .and_then(|mut segments| segments.find(|segment| !segment.is_empty()))
        .filter(|segment| *segment != "events")
        .ok_or_else(|| anyhow::anyhow!("unable to find Meetup group name in '{url}'"))?
        .to_owned();

    Ok(MeetupGroup {
        url: parsed,
        url_name,
        location_override,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_group_name_and_override() {
        let group = parse_group(
            "https://www.meetup.com/rust-noris/events/",
            Some(LocationOverride::Virtual),
        )
        .unwrap();

        assert_eq!(group.url_name, "rust-noris");
        assert_eq!(group.location_override, Some(LocationOverride::Virtual));
    }

    #[test]
    fn rejects_non_meetup_hosts() {
        assert!(parse_group("https://example.com/rust", None).is_err());
    }

    #[test]
    fn reads_existing_group_configuration() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("meetup-automation/groups/rust-meetups.json");

        let groups = read_groups(&path).unwrap();

        assert!(groups.len() > 100);
        assert!(groups.iter().any(|group| {
            group.url_name == "vancouver-rust"
                && group.location_override == Some(LocationOverride::Hybrid)
        }));
    }
}
