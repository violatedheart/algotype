use algotype::{
    app::{App, Screen},
    model,
    store::{Store, Writer},
    ui,
};
use anyhow::{Context, Result, bail};
use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io::{self, IsTerminal},
    path::PathBuf,
    time::{Duration, Instant},
};

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}
fn main() -> Result<()> {
    let mut data_dir = None;
    let mut summary = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    args.next().context("--data-dir requires a path")?,
                ))
            }
            "--summary" => summary = true,
            "--version" | "-V" => {
                println!("algotype {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!(
                    "Algotype — a quiet place to build fast hands\n\nUsage: algotype [--data-dir PATH] [--summary]\n\n  --data-dir PATH   Use a separate local profile\n  --summary         Print saved progress without opening the TUI\n  --version         Print version\n\nIn the app: Tab chooses a mode. F1 opens field notes. Ctrl+C exits.\nAll practice is offline. No accounts, telemetry, or network requests."
                );
                return Ok(());
            }
            _ => bail!("Unknown argument {arg:?}. Use --help."),
        }
    }
    let dir = data_dir
        .or_else(|| std::env::var_os("ALGOTYPE_DATA_DIR").map(PathBuf::from))
        .or_else(|| {
            directories::ProjectDirs::from("dev", "algotype", "algotype")
                .map(|d| d.data_local_dir().into())
        })
        .context("No data directory found; use --data-dir PATH")?;
    let store = Store::open(&dir)?;
    let profile = store.load()?;
    if summary {
        println!(
            "Algotype · {} sessions · {:.0} best wpm · {} day streak\nData: {}",
            profile.total_sessions,
            profile.best(),
            profile.streak(model::unix_now()),
            dir.display()
        );
        if let Some(s) = profile.sessions.last() {
            println!(
                "Latest: {} · {:.1} wpm · {:.1}% accuracy",
                s.mode, s.wpm, s.accuracy
            );
        }
        return Ok(());
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("Algotype needs an interactive terminal. Use --summary for saved progress.");
    }
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _guard = TerminalGuard;
        drop(_guard);
        previous_hook(info);
    }));
    terminal::enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.hide_cursor()?;
    let mut writer = Writer::new(store);
    let mut app = App::new(profile);
    let start = Instant::now();
    let mut last = start;
    let mut dirty = true;
    let mut input_events = 0usize;
    while !app.quit {
        let now = Instant::now();
        let seconds = start.elapsed().as_secs_f64();
        app.tick(seconds, now.duration_since(last).as_secs_f64(), &writer);
        last = now;
        if dirty {
            terminal.draw(|f| ui::draw(f, &app))?;
        }
        let running = app.screen == Screen::Practice && app.run.started_at.is_some();
        let reveal = app.screen == Screen::Results && seconds - app.session_finished_at < 1.;
        let animate = running
            || app.saving
            || (!app.profile.preferences.reduced_motion
                && (app.screen == Screen::Practice || reveal));
        let timeout = if running || (reveal && !app.profile.preferences.reduced_motion) {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(250)
        };
        dirty = animate;
        if event::poll(timeout)? {
            // Bounded draining prioritizes input without starving the timer or renderer.
            loop {
                match event::read()? {
                    Event::Key(key) if key.kind != KeyEventKind::Release => {
                        app.key(key, start.elapsed().as_secs_f64(), &writer);
                        dirty = true;
                    }
                    Event::Resize(_, _) => dirty = true,
                    Event::Paste(_) => {
                        if !app.save_failed {
                            app.notice = Some(
                                "Paste is paused here. Let your hands do the learning.".into(),
                            );
                        }
                        dirty = true;
                    }
                    _ => {}
                }
                input_events += 1;
                if app.quit || input_events >= 64 || !event::poll(Duration::ZERO)? {
                    input_events = 0;
                    break;
                }
            }
        }
    }
    writer.finish()?;
    if app.save_failed {
        bail!(
            "{}",
            app.notice
                .as_deref()
                .unwrap_or("Progress could not be saved")
        );
    }
    Ok(())
}
