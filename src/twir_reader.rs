use crate::event_line_types::{EventLineType, LineParseError};

#[derive(Clone, Debug, PartialEq, Eq)]
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
pub struct TwirLineError<'a> {
    error: LineParseError,
    line_num: u64,
    line_raw: &'a str,
}

impl std::fmt::Display for TwirLineError<'_> {
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
    type Item = Result<TwirLine<'a>, TwirLineError<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.contents.is_empty() {
            return None;
        }

        self.line_num += 1;

        let line = match self.contents.find('\n') {
            Some(offset) => {
                let line = &self.contents[..offset];
                // leave our our newline
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
                line_raw: line,
            }),
        })
    }
}
