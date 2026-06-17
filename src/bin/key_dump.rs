//! Minimal raw key event dumper.
//! Run with: cargo run --bin key_dump
//! Press any key combination to see what crossterm reports.
//! Press Ctrl+C or 'q' to quit.

use std::io::{self, Write};

use crossterm::event::{
    self, Event, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

fn main() -> io::Result<()> {
    enable_raw_mode()?;

    let enhanced = execute!(
        io::stdout(),
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
        )
    )
    .is_ok();

    println!("=== Key Dump (enhanced={enhanced}) ===\r");
    println!("Press key combos to see raw events. Press 'q' alone to quit.\r");
    println!("Try: Ctrl+Alt+=  Ctrl+Alt+-  Ctrl+Alt+h  Ctrl+Alt+v\r");
    println!("-----------------------------------------------\r");
    io::stdout().flush()?;

    loop {
        let event = event::read()?;
        match &event {
            Event::Key(key_event) => {
                println!(
                    "KeyEvent {{ code: {:?}, modifiers: {:?}, kind: {:?}, state: {:?} }}\r",
                    key_event.code, key_event.modifiers, key_event.kind, key_event.state
                );
                io::stdout().flush()?;

                if key_event.kind == event::KeyEventKind::Press
                    && matches!(key_event.code, event::KeyCode::Char('q'))
                    && key_event.modifiers.is_empty()
                {
                    break;
                }
            }
            other => {
                println!("OtherEvent: {other:?}\r");
                io::stdout().flush()?;
            }
        }
    }

    if enhanced {
        let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
    }
    disable_raw_mode()?;
    println!("\nDone.");
    Ok(())
}
