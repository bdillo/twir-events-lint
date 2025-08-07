use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
pub struct Args {
    /// TWIR draft markdown file to lint
    #[arg(short, long)]
    draft: PathBuf,
    /// File containing new TWIR events
    #[arg(short, long)]
    new_events_file: Option<PathBuf>,
    /// Enable debug logging
    #[arg(long, default_value_t = false)]
    debug: bool,
    /// Error limit before bailing - otherwise you could have a lot of output if the linter gets in a weird state
    #[arg(short = 'l', long, default_value_t = 20)]
    error_limit: u16,
}

impl Args {
    pub fn draft(&self) -> &PathBuf {
        &self.draft
    }

    pub fn new_events_file(&self) -> &Option<PathBuf> {
        &self.new_events_file
    }

    pub fn debug(&self) -> bool {
        self.debug
    }

    pub fn error_limit(&self) -> u16 {
        self.error_limit
    }
}
