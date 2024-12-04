use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
pub struct Args {
    /// Markdown file to lint
    #[arg(short, long)]
    file: PathBuf,
    /// Enable debug logging
    #[arg(short, long, default_value_t = false)]
    debug: bool,
    /// Make edits to the file - the file itself isn't altered but the new draft with edits is printed to stdout
    #[arg(short, long, default_value_t = false)]
    edit: bool,
    /// Error limit before bailing - otherwise you could have a lot of output if the linter gets in a weird state
    #[arg(short = 'l', long, default_value_t = 20)]
    error_limit: u32,
}

impl Args {
    pub fn file(&self) -> &PathBuf {
        &self.file
    }

    pub fn debug(&self) -> bool {
        self.debug
    }

    pub fn edit(&self) -> bool {
        self.edit
    }

    pub fn error_limit(&self) -> u32 {
        self.error_limit
    }
}
