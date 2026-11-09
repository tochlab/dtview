mod app;
mod dtb;
mod ui;

use std::fs;
use std::io;
use std::time::Duration;

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::app::App;
use crate::dtb::parser;

fn main() -> io::Result<()> {
    let path = std::env::args().nth(1).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "usage: dtview <path-to.dtb>")
    })?;

    let buf = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("dtview: cannot read {}: {e}", path);
            std::process::exit(1);
        }
    };

    let dtb = match parser::parse(&buf) {
        Ok(dtb) => dtb,
        Err(e) => {
            eprintln!("dtview: failed to parse {}: {e}", path);
            std::process::exit(1);
        }
    };

    let mut app = App::new(dtb);
    run_tui(&mut app)?;
    Ok(())
}

fn run_tui(app: &mut App) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let restore = || {
        disable_raw_mode().ok();
        execute!(io::stdout(), LeaveAlternateScreen).ok();
    };

    loop {
        terminal.draw(|f| ui::draw(f, app))?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if key.code == event::KeyCode::Char('q') || key.code == event::KeyCode::Esc {
                break;
            }
            app.handle_key(key);
        }
    }

    restore();
    Ok(())
}
