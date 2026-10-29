use crate::{
    adaptive,
    app::{App, Screen},
    engine::{MODES, Mode},
    model::unix_now,
};
use ratatui::{
    prelude::*,
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph},
};

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color,
    pub text: Color,
    pub muted: Color,
    pub dim: Color,
    pub accent: Color,
    pub good: Color,
    pub error: Color,
    pub name: &'static str,
}
pub const PALETTES: [Palette; 3] = [
    Palette {
        bg: Color::Rgb(20, 23, 28),
        text: Color::Rgb(222, 225, 224),
        muted: Color::Rgb(136, 148, 157),
        dim: Color::Rgb(62, 73, 84),
        accent: Color::Rgb(238, 188, 111),
        good: Color::Rgb(151, 196, 168),
        error: Color::Rgb(238, 127, 131),
        name: "Afterglow",
    },
    Palette {
        bg: Color::Rgb(19, 27, 29),
        text: Color::Rgb(218, 231, 222),
        muted: Color::Rgb(127, 155, 149),
        dim: Color::Rgb(52, 77, 73),
        accent: Color::Rgb(161, 211, 173),
        good: Color::Rgb(218, 204, 151),
        error: Color::Rgb(237, 145, 130),
        name: "Botanical",
    },
    Palette {
        bg: Color::Rgb(238, 233, 222),
        text: Color::Rgb(48, 53, 57),
        muted: Color::Rgb(102, 107, 107),
        dim: Color::Rgb(185, 182, 172),
        accent: Color::Rgb(152, 85, 44),
        good: Color::Rgb(54, 112, 86),
        error: Color::Rgb(174, 56, 68),
        name: "Paper",
    },
];
fn text(f: &mut Frame, area: Rect, value: impl Into<String>, color: Color) {
    f.render_widget(
        Paragraph::new(value.into()).style(Style::default().fg(color)),
        area.intersection(f.area()),
    );
}
fn row(f: &mut Frame, x: u16, y: u16, w: u16, value: impl Into<String>, color: Color) {
    text(f, Rect::new(x, y, w, 1), value, color);
}
fn line(f: &mut Frame, area: Rect, spans: Vec<Span<'static>>) {
    f.render_widget(
        Paragraph::new(Line::from(spans)),
        area.intersection(f.area()),
    );
}
fn label(s: impl Into<String>, c: Color) -> Span<'static> {
    Span::styled(s.into(), Style::default().fg(c))
}
fn section(f: &mut Frame, area: Rect, title: &str, p: Palette) {
    row(f, area.x, area.y, area.width, title, p.accent);
}

pub fn draw(f: &mut Frame, app: &App) {
    let p = PALETTES[app.profile.preferences.theme % 3];
    let all = f.area();
    f.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(p.bg).fg(p.text)),
        all,
    );
    if all.width < 44 || all.height < 18 {
        let y = all.height / 2;
        row(
            f,
            2,
            y.saturating_sub(1),
            all.width.saturating_sub(4),
            "algotype",
            p.accent,
        );
        row(
            f,
            2,
            y,
            all.width.saturating_sub(4),
            "A little more room to breathe.",
            p.text,
        );
        row(
            f,
            2,
            y + 1,
            all.width.saturating_sub(4),
            "Resize to 44 × 18 · Ctrl+C to exit",
            p.muted,
        );
        return;
    }
    let width = (all.width - 8).min(100);
    let x = (all.width - width) / 2;
    let content = Rect::new(x, 4, width, all.height - 7);
    line(
        f,
        Rect::new(x, 1, width, 1),
        vec![
            label("◌  ", p.accent),
            Span::styled("a l g o t y p e", Style::default().fg(p.text).bold()),
            label("    a quiet place to build fast hands", p.muted),
        ],
    );
    let right = match app.screen {
        Screen::Practice => "01 / practice",
        Screen::Results => "02 / reflection",
        Screen::Insights => "03 / your atlas",
        Screen::Settings => "04 / atmosphere",
        Screen::Help => "field notes",
    };
    if width > 75 {
        row(f, x + width - 18, 1, 18, right, p.muted);
    }
    row(f, x, 2, width, "─".repeat(width as usize), p.dim);
    match app.screen {
        Screen::Practice => practice(f, content, app, p),
        Screen::Results => results(f, content, app, p),
        Screen::Insights => insights(f, content, app, p),
        Screen::Settings => settings(f, content, app, p),
        Screen::Help => help(f, content, p),
    }
    let footer = match app.screen {
        Screen::Practice if app.run.started_at.is_some() => {
            "esc  reset       ctrl+w  erase word       ctrl+c  exit"
        }
        Screen::Practice => "tab  mode    f2  atlas    f3  atmosphere    f1  help    ctrl+c  exit",
        Screen::Results => "enter  next session    i  atlas    s  atmosphere    ctrl+c  exit",
        Screen::Insights => "↑↓  history    enter  practice    ctrl+c  exit",
        Screen::Settings => "t  palette    m  motion    tab  mode    enter  practice",
        Screen::Help => "enter  practice    ctrl+c  exit",
    };
    row(f, x, all.height - 2, width, footer, p.muted);
    if let Some(notice) = &app.notice {
        row(
            f,
            x,
            all.height - 3,
            width,
            notice,
            if app.save_failed { p.error } else { p.muted },
        );
    }
}

fn practice(f: &mut Frame, a: Rect, app: &App, p: Palette) {
    let running = app.run.started_at.is_some();
    let modes = MODES
        .iter()
        .enumerate()
        .map(|(i, m)| {
            label(
                format!(
                    "{}{}  ",
                    if i == app.profile.preferences.mode {
                        "• "
                    } else {
                        "  "
                    },
                    m.label()
                ),
                if i == app.profile.preferences.mode {
                    p.accent
                } else {
                    p.dim
                },
            )
        })
        .collect();
    line(f, Rect::new(a.x, a.y, a.width, 1), modes);
    let y = a.y + if a.height > 19 { 4 } else { 2 };
    let status = if running {
        "F L O W   S T A T E"
    } else {
        "L E T   T H E   W O R L D   G O   Q U I E T"
    };
    row(f, a.x, y, a.width, status, p.muted);
    let time = match app.run.mode {
        Mode::Time(s) => format!(
            "{:02}s",
            (s as f64 - app.run.elapsed(app.now)).ceil().max(0.) as u64
        ),
        _ => format!(
            "{}/{}",
            app.run.typed.iter().filter(|c| **c == ' ').count(),
            app.run.mode.words()
        ),
    };
    line(
        f,
        Rect::new(a.x, y + 2, a.width, 1),
        vec![
            label(format!("{:>3.0}", app.display_wpm), p.accent),
            label(" wpm     ", p.muted),
            label(format!("{:>5.1}%", app.run.accuracy()), p.text),
            label(" accuracy     ", p.muted),
            label(time, p.accent),
        ],
    );
    let surface_y = y + 5;
    let surface_h = if a.height > 18 { 5 } else { 3 };
    typing(f, Rect::new(a.x, surface_y, a.width, surface_h), app, p);
    let under = surface_y + surface_h + 1;
    if under < a.bottom() {
        let cells = (a.width as f64 * app.run.progress(app.now)).round() as usize;
        line(
            f,
            Rect::new(a.x, under, a.width, 1),
            vec![
                label("━".repeat(cells), p.accent),
                label("─".repeat(a.width as usize - cells), p.dim),
            ],
        );
    }
    if under + 2 < a.bottom() {
        let message = if !running {
            "begin anywhere. the clock starts with you."
        } else if app.run.accuracy() < 90. {
            "ease the pace. precision is the path to speed."
        } else {
            "soft shoulders. steady hands. one word at a time."
        };
        row(f, a.x, under + 2, a.width, message, p.muted);
    }
    if under + 4 < a.bottom() {
        row(f, a.x, under + 4, a.width, &app.plan.explanation, p.accent);
    }
    if !running && under + 6 < a.bottom() {
        row(
            f,
            a.x,
            under + 6,
            a.width,
            format!(
                "LOCAL FIRST   ·   {} sessions   ·   {} day streak   ·   no accounts, no noise",
                app.profile.total_sessions,
                app.profile.streak(unix_now())
            ),
            p.dim,
        );
    }
}

/// Wrap at word boundaries while retaining each space's input position.
fn wrapped(chars: &[char], width: usize) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut start = 0;
    while start < chars.len() {
        let max = (start + width).min(chars.len());
        let end = if max < chars.len() {
            chars[start..max]
                .iter()
                .rposition(|c| *c == ' ')
                .map_or(max, |n| start + n + 1)
        } else {
            max
        };
        out.push((start, end));
        start = end;
    }
    out
}
fn typing(f: &mut Frame, a: Rect, app: &App, p: Palette) {
    let lines = wrapped(&app.run.target, a.width.max(1) as usize);
    let cursor = app.run.typed.len();
    let active = lines
        .iter()
        .position(|(s, e)| cursor >= *s && cursor < *e)
        .unwrap_or(lines.len().saturating_sub(1));
    let first = active.saturating_sub(1);
    for (r, (start, end)) in lines.iter().enumerate().skip(first).take(a.height as usize) {
        let y = a.y + (r - first) as u16;
        for (column, index) in (*start..*end).enumerate() {
            let expected = app.run.target[index];
            let mut style = Style::default().fg(if r == active { p.muted } else { p.dim });
            let mut shown = expected;
            if let Some(actual) = app.run.typed.get(index) {
                if *actual == expected {
                    style = style.fg(p.text);
                } else {
                    style = style.fg(p.error).underlined();
                    if expected == ' ' {
                        shown = '·';
                    }
                }
            }
            if index == cursor {
                let blink = app.profile.preferences.reduced_motion
                    || (app.now * 2.).sin() > -0.7
                    || app.run.started_at.is_some();
                style = style
                    .bg(if blink { p.accent } else { p.muted })
                    .fg(p.bg)
                    .bold();
                if std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty()) {
                    style = Style::default().reversed().bold();
                }
                if expected == ' ' {
                    shown = ' ';
                }
            }
            f.buffer_mut()[(a.x + column as u16, y)]
                .set_char(shown)
                .set_style(style);
        }
    }
}
const DIGITS: [[&str; 5]; 10] = [
    ["███", "█ █", "█ █", "█ █", "███"],
    [" █ ", "██ ", " █ ", " █ ", "███"],
    ["███", "  █", "███", "█  ", "███"],
    ["███", "  █", "███", "  █", "███"],
    ["█ █", "█ █", "███", "  █", "  █"],
    ["███", "█  ", "███", "  █", "███"],
    ["███", "█  ", "███", "█ █", "███"],
    ["███", "  █", "  █", " █ ", " █ "],
    ["███", "█ █", "███", "█ █", "███"],
    ["███", "█ █", "███", "  █", "███"],
];
fn big_number(f: &mut Frame, x: u16, y: u16, value: u64, p: Palette) {
    let s = value.min(999).to_string();
    for (column, c) in s.chars().enumerate() {
        for (r, line) in DIGITS[c.to_digit(10).unwrap() as usize].iter().enumerate() {
            row(f, x + column as u16 * 4, y + r as u16, 3, *line, p.accent);
        }
    }
}
fn graph(f: &mut Frame, a: Rect, values: &[f64], p: Palette) {
    if values.len() < 2 {
        row(
            f,
            a.x,
            a.y,
            a.width,
            "A longer session will reveal your rhythm here.",
            p.muted,
        );
        return;
    }
    let data: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, *v))
        .collect();
    let max = values.iter().copied().fold(1., f64::max) * 1.15;
    let dataset = Dataset::default()
        .data(&data)
        .graph_type(GraphType::Line)
        .marker(symbols::Marker::Braille)
        .style(Style::default().fg(p.accent));
    let chart = Chart::new(vec![dataset])
        .style(Style::default().bg(p.bg))
        .x_axis(
            Axis::default()
                .bounds([0., (values.len() - 1) as f64])
                .style(Style::default().fg(p.dim)),
        )
        .y_axis(
            Axis::default()
                .bounds([0., max])
                .style(Style::default().fg(p.dim)),
        );
    f.render_widget(chart, a);
}
fn results(f: &mut Frame, a: Rect, app: &App, p: Palette) {
    let Some(s) = &app.result else {
        return;
    };
    section(
        f,
        a,
        if app.new_best && app.profile.total_sessions > 1 {
            "A NEW HIGH WATER MARK."
        } else {
            "A LITTLE MORE FLUENT THAN BEFORE."
        },
        p,
    );
    let reveal = if app.profile.preferences.reduced_motion {
        1.
    } else {
        ((app.now - app.session_finished_at) / 0.7).clamp(0., 1.)
    };
    big_number(
        f,
        a.x,
        a.y + 2,
        (s.wpm * (1. - (1. - reveal).powi(3))).round() as u64,
        p,
    );
    row(
        f,
        a.x + 15,
        a.y + 3,
        a.width - 15,
        "words per minute",
        p.muted,
    );
    row(
        f,
        a.x + 15,
        a.y + 5,
        a.width - 15,
        format!(
            "{}  ·  {:.1}s  ·  {} keystrokes",
            s.mode, s.seconds, s.attempts
        ),
        p.muted,
    );
    let y = a.y + 8;
    let w = a.width / 3;
    for (i, (value, name)) in [
        (format!("{:.1}%", s.accuracy), "ACCURACY"),
        (format!("{:.0}%", s.consistency), "RHYTHM"),
        (format!("{:.0}", s.raw_wpm), "RAW WPM"),
    ]
    .iter()
    .enumerate()
    {
        row(f, a.x + i as u16 * w, y, w, value, p.text);
        row(f, a.x + i as u16 * w, y + 1, w, *name, p.muted);
    }
    if a.height > 16 {
        let chart_h = (a.height - 15).clamp(3, 7);
        graph(f, Rect::new(a.x, y + 3, a.width, chart_h), &s.trace, p);
        let bottom = y + chart_h + 4;
        if bottom < a.bottom() {
            row(
                f,
                a.x,
                bottom,
                a.width,
                format!(
                    "{} mistakes   ·   {} deletions   ·   {}",
                    s.errors,
                    s.corrections,
                    if app.saving {
                        "saving locally…"
                    } else if app.save_failed {
                        "SAVE FAILED · r to retry"
                    } else {
                        "saved on this device"
                    }
                ),
                if app.save_failed { p.error } else { p.muted },
            );
        }
        if bottom + 2 < a.bottom() {
            let message = if s.accuracy < 95. {
                "NEXT / Slow down a little. Aim for 97% before chasing speed.".into()
            } else if !s.targets.is_empty() {
                format!(
                    "NEXT / Revisit {} with a fresh set of words.",
                    s.targets.join(" · ")
                )
            } else {
                "NEXT / Your baseline is taking shape. Keep your rhythm easy.".into()
            };
            row(f, a.x, bottom + 2, a.width, message, p.accent);
        }
    }
}

fn insights(f: &mut Frame, a: Rect, app: &App, p: Palette) {
    section(f, a, "YOUR HANDS HAVE A HISTORY.", p);
    if app.profile.sessions.is_empty() {
        row(
            f,
            a.x,
            a.y + 3,
            a.width,
            "Your atlas starts with a single session.",
            p.text,
        );
        row(
            f,
            a.x,
            a.y + 5,
            a.width,
            "Practice naturally. We will map accuracy, timing,",
            p.muted,
        );
        row(
            f,
            a.x,
            a.y + 6,
            a.width,
            "and patterns that deserve a little more attention.",
            p.muted,
        );
        return;
    }
    let n = app.profile.sessions.len();
    let latest: Vec<_> = app.profile.sessions.iter().rev().take(10).collect();
    let avg = latest.iter().map(|s| s.wpm).sum::<f64>() / latest.len() as f64;
    row(
        f,
        a.x,
        a.y + 2,
        a.width,
        format!(
            "{:.0} best wpm     {:.0} recent average     {} day streak     {} sessions",
            app.profile.best(),
            avg,
            app.profile.streak(unix_now()),
            app.profile.total_sessions
        ),
        p.text,
    );
    if a.height < 20 {
        weakness_table(f, Rect::new(a.x, a.y + 5, a.width, a.height - 5), app, p);
        return;
    }
    let history: Vec<_> = app
        .profile
        .sessions
        .iter()
        .skip(n.saturating_sub(30))
        .map(|s| s.wpm)
        .collect();
    if history.len() < 2 {
        row(
            f,
            a.x,
            a.y + 4,
            a.width,
            "Finish another session to reveal your progress curve.",
            p.muted,
        );
    } else {
        graph(f, Rect::new(a.x, a.y + 4, a.width, 4), &history, p);
    }
    let y = a.y + 10;
    if a.width >= 76 {
        let left = a.width / 2;
        weakness_table(
            f,
            Rect::new(a.x, y, left, a.bottom().saturating_sub(y)),
            app,
            p,
        );
        row(
            f,
            a.x + left + 3,
            y,
            a.width - left - 3,
            "THE KEY GARDEN",
            p.accent,
        );
        for (r, keys) in ["qwertyuiop", "asdfghjkl", "zxcvbnm"].iter().enumerate() {
            for (c, k) in keys.chars().enumerate() {
                let skill = app.profile.skills.get(&k.to_string());
                let color = skill.map_or(p.dim, |s| {
                    if s.attempts < 8 {
                        p.muted
                    } else if adaptive::mastery(s, unix_now()) > 65. {
                        p.good
                    } else {
                        p.accent
                    }
                });
                row(
                    f,
                    a.x + left + 3 + c as u16 * 3 + r as u16,
                    y + 2 + r as u16 * 2,
                    2,
                    k.to_string(),
                    color,
                );
            }
        }
        row(
            f,
            a.x + left + 3,
            y + 9,
            a.width - left - 3,
            "muted: new   amber: growing",
            p.muted,
        );
        row(
            f,
            a.x + left + 3,
            y + 10,
            a.width - left - 3,
            "green: established + retained",
            p.muted,
        );
    } else {
        weakness_table(
            f,
            Rect::new(a.x, y, a.width, a.bottom().saturating_sub(y)),
            app,
            p,
        );
    }
    if a.height >= 26 {
        let offset = app.history_offset.min(n - 1);
        let s = &app.profile.sessions[n - 1 - offset];
        row(
            f,
            a.x,
            a.bottom() - 2,
            a.width,
            format!(
                "SESSION {:03}   {}   {:.0} wpm   {:.1}% accuracy   {} mistakes",
                n - offset,
                s.mode,
                s.wpm,
                s.accuracy,
                s.errors
            ),
            p.text,
        );
    }
}
fn weakness_table(f: &mut Frame, a: Rect, app: &App, p: Palette) {
    row(f, a.x, a.y, a.width, "WORTH YOUR ATTENTION", p.accent);
    row(
        f,
        a.x,
        a.y + 2,
        a.width,
        "pattern   est. acc   delay   recall",
        p.muted,
    );
    let ranked = adaptive::weaknesses(&app.profile, unix_now());
    if ranked.is_empty() {
        row(
            f,
            a.x,
            a.y + 4,
            a.width,
            "Gathering evidence. Keep practicing.",
            p.muted,
        );
    }
    for (i, s) in ranked
        .iter()
        .take(a.height.saturating_sub(5).min(6) as usize)
        .enumerate()
    {
        row(
            f,
            a.x,
            a.y + 4 + i as u16,
            a.width,
            format!(
                "{:<7}   {:>5.1}%   {:>3.0}ms   {:>3.0}%",
                s.gram,
                s.accuracy,
                s.latency_ms,
                s.retention * 100.
            ),
            if i == 0 { p.text } else { p.muted },
        );
    }
}
fn settings(f: &mut Frame, a: Rect, app: &App, p: Palette) {
    section(f, a, "MAKE YOURSELF AT HOME.", p);
    row(
        f,
        a.x,
        a.y + 2,
        a.width,
        "01  /  PALETTE                                      t",
        p.muted,
    );
    for (i, palette) in PALETTES.iter().enumerate() {
        line(
            f,
            Rect::new(a.x + 2, a.y + 4 + i as u16, a.width - 2, 1),
            vec![
                label(
                    if app.profile.preferences.theme == i {
                        "●  "
                    } else {
                        "○  "
                    },
                    p.accent,
                ),
                label(
                    format!("{:<16}", palette.name),
                    if app.profile.preferences.theme == i {
                        p.text
                    } else {
                        p.muted
                    },
                ),
                label("● ", palette.accent),
                label("● ", palette.good),
                label("● ", palette.error),
            ],
        );
    }
    row(
        f,
        a.x,
        a.y + 8,
        a.width,
        "02  /  MOTION                                       m",
        p.muted,
    );
    row(
        f,
        a.x + 2,
        a.y + 9,
        a.width - 2,
        if app.profile.preferences.reduced_motion {
            "Still · no transitions or blinking cursor"
        } else {
            "Flow · eased counters, quiet cursor, soft reveals"
        },
        p.text,
    );
    if a.height > 15 {
        row(
            f,
            a.x,
            a.y + 13,
            a.width,
            "03  /  DEFAULT PRACTICE                           tab",
            p.muted,
        );
        row(
            f,
            a.x + 2,
            a.y + 15,
            a.width - 2,
            MODES[app.profile.preferences.mode].label(),
            p.text,
        );
    }
    if a.height > 19 {
        row(
            f,
            a.x,
            a.y + 19,
            a.width,
            "Preferences are saved automatically, on this device.",
            p.dim,
        );
    }
}
fn help(f: &mut Frame, a: Rect, p: Palette) {
    section(f, a, "FIELD NOTES / FIND YOUR FLOW", p);
    for (i, (key, meaning)) in [
        ("type", "Start the clock with your first character."),
        ("backspace", "Correct a character. Mistakes still count."),
        ("ctrl+w", "Erase the current word."),
        (
            "esc / ctrl+r",
            "Reset an unfinished session; no result is saved.",
        ),
        ("tab", "Before typing: choose time, words, or focus."),
        ("f2 / f3", "Before typing: your atlas / atmosphere."),
        ("ctrl+c / ctrl+q", "Exit safely, flushing saved progress."),
    ]
    .iter()
    .enumerate()
    {
        line(
            f,
            Rect::new(
                a.x,
                a.y + 3 + i as u16 * if a.height < 20 { 1 } else { 2 },
                a.width,
                1,
            ),
            vec![
                label(format!("{key:<17}"), p.accent),
                label(*meaning, p.text),
            ],
        );
    }
    if a.height > 22 {
        row(
            f,
            a.x,
            a.y + 19,
            a.width,
            "WPM = correct characters / 5 / minutes. Accuracy includes",
            p.muted,
        );
        row(
            f,
            a.x,
            a.y + 20,
            a.width,
            "corrected mistakes. Rhythm measures variation per second.",
            p.muted,
        );
        row(
            f,
            a.x,
            a.y + 22,
            a.width,
            "No paste shortcuts. No network. No rush.",
            p.accent,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Profile;
    use ratatui::{Terminal, backend::TestBackend};
    #[test]
    fn every_screen_and_theme_renders_at_supported_sizes() {
        for (w, h) in [(12, 5), (44, 18), (80, 24), (120, 36), (180, 50)] {
            let mut app = App::new(Profile::default());
            app.result = Some(app.run.record(1., 1, vec![]));
            for theme in 0..3 {
                app.profile.preferences.theme = theme;
                for screen in [
                    Screen::Practice,
                    Screen::Results,
                    Screen::Insights,
                    Screen::Settings,
                    Screen::Help,
                ] {
                    app.screen = screen;
                    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
                    terminal.draw(|f| draw(f, &app)).unwrap();
                    assert!(
                        terminal
                            .backend()
                            .buffer()
                            .content
                            .iter()
                            .any(|c| c.symbol() != " ")
                    );
                }
            }
        }
    }
    #[test]
    fn word_wrapping_preserves_every_input_position() {
        let chars: Vec<_> = "one two three four".chars().collect();
        let lines = wrapped(&chars, 8);
        let rebuilt: String = lines
            .iter()
            .flat_map(|(s, e)| chars[*s..*e].iter())
            .collect();
        assert_eq!(rebuilt, "one two three four");
        assert!(lines.iter().all(|(s, e)| e - s <= 8));
    }
}
