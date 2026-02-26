mod types;
mod horizons;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io::{self, Stdout},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::types::{AppState, BodyState, BODIES, FOCUS_LEVELS};
use crate::horizons::updater;
use crate::ui::draw_ui;

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn has_arg(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}

fn clamp_zoom(z: f64) -> f64 {
    z.clamp(0.2, 50.0)
}

#[tokio::main]
async fn main() -> Result<()> {
    let use_unicode_icons = has_arg("--unicode");

    let bodies = BODIES
        .iter()
        .map(|m| BodyState { name: m.name, id: m.id, pos_au: None })
        .collect::<Vec<_>>();

    let state = Arc::new(Mutex::new(AppState {
        bodies,
        last_update_utc: None,
        status: "Starting…".into(),
        use_unicode_icons,
        zoom: 1.0,
        focus_index: FOCUS_LEVELS.len() - 1, // default: Neptune fit
    }));

    tokio::spawn(updater(state.clone()));

    let mut terminal = setup_terminal()?;

    loop {
        let snapshot = { state.lock().unwrap().clone() };
        terminal.draw(|f| draw_ui(f, &snapshot))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(k) = event::read()? {
                match k.code {
                    KeyCode::Char('q') => break,

                    // zoom in
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        let mut s = state.lock().unwrap();
                        s.zoom = clamp_zoom(s.zoom * 1.25);
                    }
                    // zoom out
                    KeyCode::Char('-') => {
                        let mut s = state.lock().unwrap();
                        s.zoom = clamp_zoom(s.zoom / 1.25);
                    }
                    // reset zoom
                    KeyCode::Char('0') => {
                        let mut s = state.lock().unwrap();
                        s.zoom = 1.0;
                        s.focus_index = FOCUS_LEVELS.len() - 1;
                    }
                    // focus in reminder: smaller max orbit
                    KeyCode::Char('[') => {
                        let mut s = state.lock().unwrap();
                        if s.focus_index > 0 {
                            s.focus_index -= 1;
                        }
                    }
                    // focus out: larger max orbit
                    KeyCode::Char(']') => {
                        let mut s = state.lock().unwrap();
                        if s.focus_index + 1 < FOCUS_LEVELS.len() {
                            s.focus_index += 1;
                        }
                    }

                    _ => {}
                }
            }
        }
    }

    restore_terminal(terminal)?;
    Ok(())
}
