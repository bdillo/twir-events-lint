use std::{error::Error, fs};

use log::info;
use twir_events_lint::lint::EventSectionLinter;

fn main() -> Result<(), Box<dyn Error>> {
    simple_logger::init_with_level(log::Level::Debug).expect("Failed to init logger!");

    // TODO: add clap, etc
    let md = fs::read_to_string("./test/570.md")?;

    let mut event_linter = EventSectionLinter::default();
    event_linter.lint(&md)?;

    info!("LGTM!");

    Ok(())
}
