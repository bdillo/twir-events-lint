use std::{borrow::Cow, collections::HashMap};

use chrono::NaiveDate;
use url::Url;

use crate::line_types::{EventLineType, LineParseError};

/// A single line from
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line<'a> {
    line_num: u64,
    line_type: EventLineType,
    line_raw: Cow<'a, str>,
}

impl<'a> Line<'a> {
    pub fn into_owned(self) -> Line<'static> {
        Line {
            line_num: self.line_num,
            line_type: self.line_type.clone(),
            line_raw: Cow::Owned(self.line_raw.into_owned()),
        }
    }

    pub fn get_line_num(&self) -> u64 {
        self.line_num
    }

    pub fn get_line_type(&self) -> &EventLineType {
        &self.line_type
    }

    pub fn get_line_raw(&self) -> &Cow<'a, str> {
        &self.line_raw
    }
}

impl std::fmt::Display for Line<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line #{}, type '{}': '{}'",
            self.line_num, self.line_type, self.line_raw
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineError {
    error: LineParseError,
    line_num: u64,
    line_raw: String,
}

impl std::fmt::Display for LineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "parse error: {}\nline #{}: '{}'",
            self.error, self.line_num, self.line_raw
        )
    }
}

// TODO: where to put nice interface to collect events from?
#[derive(Debug)]
pub struct Reader<'a> {
    contents: &'a str,
    current_line_num: u64,
}

impl<'a> Reader<'a> {
    pub fn new(contents: &'a str) -> Self {
        Self {
            contents,
            current_line_num: 0,
        }
    }
}

impl<'a> Iterator for Reader<'a> {
    type Item = Result<Line<'a>, LineError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.contents.is_empty() {
            return None;
        }

        self.current_line_num += 1;

        let line = match self.contents.find('\n') {
            Some(offset) => {
                let line = &self.contents[..offset];
                // leave out our newline
                self.contents = &self.contents[offset + 1..];
                line
            }
            None => self.contents,
        };

        Some(match line.parse::<EventLineType>() {
            Ok(line_type) => Ok(Line {
                line_num: self.current_line_num,
                line_type,
                line_raw: Cow::Borrowed(line),
            }),
            Err(e) => Err(LineError {
                error: e,
                line_num: self.current_line_num,
                line_raw: line.to_owned(),
            }),
        })
    }
}

/// TODO: probably move this into its own thing
/// maybe make line_types use this
#[derive(Debug)]
pub enum EventDate {
    Date(NaiveDate),
    DateRange { start: NaiveDate, end: NaiveDate },
}

#[derive(Debug)]
pub enum EventLocation {
    Virtual,
    VirtualWithLocation(String),
    InPerson,
}

#[derive(Debug)]
pub struct EventGroup {
    name: String,
    url: Url,
}

#[derive(Debug)]
pub struct Event {
    name: String,
    url: Url,
}

#[derive(Debug)]
pub struct EventListing {
    date: EventDate,
    location: EventLocation,
    event_groups: Vec<EventGroup>,
    event_instances: Vec<Event>,
}

type EventsByRegion = HashMap<String, Vec<Event>>;

pub fn get_events(reader: Reader) -> Result<EventsByRegion, LineError> {}
