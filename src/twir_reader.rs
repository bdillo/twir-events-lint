use crate::event_line_types::{EventLineType, LineParseError};

#[derive(Debug, PartialEq, Eq)]
pub struct TwirLine<'a> {
    line_num: u64,
    line_type: EventLineType,
    line_raw: &'a str,
}

impl TwirLine<'_> {
    pub fn line_num(&self) -> u64 {
        self.line_num
    }

    pub fn line_type(&self) -> &EventLineType {
        &self.line_type
    }

    pub fn line_raw(&self) -> &str {
        self.line_raw
    }
}

impl TwirLine<'_> {
    pub fn to_owned(&self) -> OwnedTwirLine {
        OwnedTwirLine {
            line_num: self.line_num,
            line_type: self.line_type.clone(),
            line_raw: self.line_raw.to_owned(),
        }
    }
}

impl std::fmt::Display for TwirLine<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line #{}, type '{}': '{}'",
            self.line_num, self.line_type, self.line_raw
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedTwirLine {
    line_num: u64,
    line_type: EventLineType,
    line_raw: String,
}

impl std::fmt::Display for OwnedTwirLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line #{}, type '{}': '{}'",
            self.line_num, self.line_type, self.line_raw
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwirLineError {
    error: LineParseError,
    line_num: u64,
    line_raw: String,
}

impl std::fmt::Display for TwirLineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "parse error: {}\nline #{}: '{}'",
            self.error, self.line_num, self.line_raw
        )
    }
}

#[derive(Debug)]
pub struct TwirReader<'a> {
    contents: &'a str,
    line_num: u64,
}

impl<'a> TwirReader<'a> {
    pub fn new(contents: &'a str) -> Self {
        Self {
            contents,
            line_num: 0,
        }
    }
}

impl<'a> Iterator for TwirReader<'a> {
    type Item = Result<TwirLine<'a>, TwirLineError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.contents.is_empty() {
            return None;
        }

        self.line_num += 1;

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
            Ok(line_type) => Ok(TwirLine {
                line_num: self.line_num,
                line_type,
                line_raw: line,
            }),
            Err(e) => Err(TwirLineError {
                error: e,
                line_num: self.line_num,
                line_raw: line.to_owned(),
            }),
        })
    }
}
