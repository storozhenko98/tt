mod app;
mod game;
mod net;
mod ui;
mod update;

use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

const TICK: Duration = Duration::from_millis(16); // ~60 fps

fn main() -> Result<()> {
    // Auto-update before doing anything else (silently skips on failure)
    update::auto_update();

    let name = std::env::args().nth(1).unwrap_or_else(|| {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "Player".into())
    });

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Restore terminal on panic
    let orig = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), LeaveAlternateScreen);
        orig(info);
    }));

    let res = run(&mut terminal, name);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    if let Err(e) = &res {
        eprintln!("Error: {e}");
    }
    res
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, name: String) -> Result<()> {
    let mut app = app::App::new(name)?;
    let dt = 1.0 / 60.0;

    loop {
        let frame_start = Instant::now();

        // Drain input events
        while event::poll(Duration::ZERO)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                    app.handle_key(key.code);
                }
            }
        }

        app.tick(dt);
        terminal.draw(|frame| ui::render(frame, &app))?;

        if app.should_quit {
            break;
        }

        // Frame pacing — sleep only the remainder of the 16ms budget
        let remaining = TICK.saturating_sub(frame_start.elapsed());
        if !remaining.is_zero() {
            let _ = event::poll(remaining);
        }
    }

    Ok(())
}
