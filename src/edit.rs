use std::{
    fmt, fs,
    io::{self, Write},
    ops::Range,
    path::Path,
};

/// An absolute UTF-8 byte range within a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSpan(Range<usize>);

impl SourceSpan {
    pub fn new(start: usize, end: usize) -> Self {
        Self(start..end)
    }

    pub fn start(&self) -> usize {
        self.0.start
    }

    pub fn end(&self) -> usize {
        self.0.end
    }
}

/// A replacement to apply to a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    span: SourceSpan,
    replacement: String,
}

impl TextEdit {
    pub fn new(span: SourceSpan, replacement: impl Into<String>) -> Self {
        Self {
            span,
            replacement: replacement.into(),
        }
    }

    pub fn delete(span: SourceSpan) -> Self {
        Self::new(span, "")
    }

    pub fn span(&self) -> &SourceSpan {
        &self.span
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    InvalidSpan(SourceSpan),
    NotCharacterBoundary(SourceSpan),
    OverlappingEdits {
        first: SourceSpan,
        second: SourceSpan,
    },
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSpan(span) => write!(
                f,
                "invalid edit span {}..{} for document",
                span.start(),
                span.end()
            ),
            Self::NotCharacterBoundary(span) => write!(
                f,
                "edit span {}..{} is not on UTF-8 character boundaries",
                span.start(),
                span.end()
            ),
            Self::OverlappingEdits { first, second } => write!(
                f,
                "edit spans {}..{} and {}..{} overlap",
                first.start(),
                first.end(),
                second.start(),
                second.end()
            ),
        }
    }
}

impl std::error::Error for EditError {}

/// Applies non-overlapping edits without modifying the input document.
pub fn apply_edits(document: &str, edits: &[TextEdit]) -> Result<String, EditError> {
    let mut edits = edits.iter().collect::<Vec<_>>();
    edits.sort_by_key(|edit| edit.span.start());

    for edit in &edits {
        let span = &edit.span;
        if span.start() > span.end() || span.end() > document.len() {
            return Err(EditError::InvalidSpan(span.clone()));
        }
        if !document.is_char_boundary(span.start()) || !document.is_char_boundary(span.end()) {
            return Err(EditError::NotCharacterBoundary(span.clone()));
        }
    }

    for pair in edits.windows(2) {
        if pair[0].span.end() > pair[1].span.start() {
            return Err(EditError::OverlappingEdits {
                first: pair[0].span.clone(),
                second: pair[1].span.clone(),
            });
        }
    }

    let mut result = document.to_owned();
    for edit in edits.into_iter().rev() {
        result.replace_range(edit.span.0.clone(), &edit.replacement);
    }
    Ok(result)
}

/// Atomically replaces a file using a temporary file in the same directory.
pub fn replace_file(path: &Path, contents: &str) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let permissions = fs::metadata(path)?.permissions();

    for sequence in 0..100 {
        let temporary_name = format!(
            ".{}.{}.{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            sequence
        );
        let temporary_path = parent.join(temporary_name);
        let mut temporary = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };

        let write_result = (|| {
            temporary.set_permissions(permissions.clone())?;
            temporary.write_all(contents.as_bytes())?;
            temporary.sync_all()?;
            drop(temporary);
            fs::rename(&temporary_path, path)
        })();

        if write_result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        return write_result;
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not create a temporary file",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_multiple_edits_using_original_offsets() {
        let document = "one two three";
        let edits = [
            TextEdit::new(SourceSpan::new(0, 3), "1"),
            TextEdit::delete(SourceSpan::new(8, 13)),
        ];

        assert_eq!(apply_edits(document, &edits).unwrap(), "1 two ");
    }

    #[test]
    fn rejects_overlapping_edits() {
        let edits = [
            TextEdit::delete(SourceSpan::new(0, 3)),
            TextEdit::delete(SourceSpan::new(2, 4)),
        ];

        assert!(matches!(
            apply_edits("test", &edits),
            Err(EditError::OverlappingEdits { .. })
        ));
    }

    #[test]
    fn rejects_spans_inside_utf8_characters() {
        let document = "🦀";
        let edit = TextEdit::delete(SourceSpan::new(1, document.len()));

        assert!(matches!(
            apply_edits(document, &[edit]),
            Err(EditError::NotCharacterBoundary(_))
        ));
    }
}
