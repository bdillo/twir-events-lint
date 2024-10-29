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
}

impl Args {
    pub fn file(&self) -> &PathBuf {
        &self.file
    }

    pub fn debug(&self) -> bool {
        self.debug
    }
}
