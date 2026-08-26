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
    /// File containing configured event sources to fetch
    #[arg(long)]
    event_sources: Option<PathBuf>,
    /// Write merged new events back to the draft
    #[arg(long, default_value_t = false)]
    in_place: bool,
    /// Remove events outside the newsletter date range
    #[arg(long, default_value_t = false)]
    fix: bool,
    /// Log API responses and event normalization decisions
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

    pub fn event_sources(&self) -> &Option<PathBuf> {
        &self.event_sources
    }

    pub fn in_place(&self) -> bool {
        self.in_place
    }

    pub fn fix(&self) -> bool {
        self.fix
    }

    pub fn debug(&self) -> bool {
        self.debug
    }

    pub fn error_limit(&self) -> u16 {
        self.error_limit
    }
}
