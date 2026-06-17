mod action;
mod app;
mod core;
mod tui;

use color_eyre::Result;

use crate::app::App;

fn main() -> Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new().run(terminal);
    ratatui::restore();
    result
}
