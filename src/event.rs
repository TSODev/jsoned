use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use std::time::Duration;

pub enum AppEvent {
    Key(KeyEvent),
    Tick,
}

pub fn next_event(tick_ms: u64) -> Result<AppEvent> {
    if event::poll(Duration::from_millis(tick_ms))? {
        if let Event::Key(key) = event::read()? {
            return Ok(AppEvent::Key(key));
        }
    }
    Ok(AppEvent::Tick)
}
