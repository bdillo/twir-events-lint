mod config;
mod location;
mod luma;
mod meetup;

use std::path::Path;

use anyhow::Context;
use chrono::NaiveDate;

use crate::events::EventsByRegion;

use self::config::read_sources;

pub struct Collection {
    pub events: EventsByRegion,
    pub warnings: Vec<String>,
}

pub fn collect(
    sources_path: &Path,
    range_start: NaiveDate,
    range_end: NaiveDate,
) -> anyhow::Result<Collection> {
    let configured = read_sources(sources_path)?;
    let mut events = EventsByRegion::new();
    let mut warnings = Vec::new();

    if !configured.meetup.is_empty() {
        let collection = meetup::collect(configured.meetup, range_start, range_end)?;
        warnings.extend(collection.warnings);
        let collected = EventsByRegion::try_from(collection.events)
            .context("failed to validate collected Meetup events")?;
        events = events.merge(&collected);
    }

    if !configured.luma.is_empty() {
        let collection = luma::collect(configured.luma, range_start, range_end)?;
        warnings.extend(collection.warnings);
        let collected = EventsByRegion::try_from(collection.events)
            .context("failed to validate collected Luma events")?;
        events = events.merge(&collected);
    }

    Ok(Collection { events, warnings })
}
