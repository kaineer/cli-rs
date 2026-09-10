use chrono::{DateTime, Local};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
    Frame,
};

use crate::app::{format_duration, App, EntryState, Panel};

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();
    app.area_width = size.width;

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(size);

    let main_area = vertical[0];
    let status_area = vertical[1];

    let max_left = main_area.width.saturating_sub(1);
    if app.left_width > max_left {
        app.left_width = max_left;
    }

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(app.left_width),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(main_area);

    draw_left(f, app, horizontal[0]);
    draw_divider(f, app, horizontal[1]);
    draw_right(f, app, horizontal[2]);
    draw_status(f, app, status_area);
}

/// Кадры спиннера (Braille Patterns, каждый символ — 1 колонка).
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn spinner_frame(tick: u64) -> &'static str {
    SPINNER[(tick as usize) % SPINNER.len()]
}

fn highlight_style_for(panel: Panel, focus: Panel) -> Style {
    if panel == focus {
        Style::default()
            .bg(Color::Rgb(0x5e, 0x81, 0xac)) // Nord blue
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .bg(Color::Rgb(0x4c, 0x56, 0x6a)) // Nord polar night 1
            .fg(Color::Gray)
    }
}

fn draw_left(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let marker: Option<(String, Color, Option<String>)> = match &app.states[i] {
                EntryState::Idle => None,
                EntryState::Running => {
                    Some((spinner_frame(app.tick).to_string(), Color::Yellow, None))
                }
                EntryState::Done { run_id } => {
                    if let Some(run) = app.runs.get(*run_id) {
                        let sym = if run.ok { "✓" } else { "✗" };
                        let color = if run.ok { Color::Green } else { Color::Red };
                        let since = run
                            .finished_at
                            .elapsed()
                            .unwrap_or(std::time::Duration::ZERO);
                        let dur = format_duration(since);
                        let dur = if dur.is_empty() { None } else { Some(dur) };
                        Some((sym.to_string(), color, dur))
                    } else {
                        None
                    }
                }
            };

            let mut spans = vec![Span::raw(e.label.clone())];
            if let Some((sym, color, dur)) = marker {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(sym, Style::default().fg(color)));
                if let Some(d) = dur {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(d, Style::default().fg(Color::DarkGray)));
                }
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(highlight_style_for(Panel::Left, app.focus))
        .highlight_symbol(" ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_divider(f: &mut Frame, app: &App, area: Rect) {
    let divider_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    let divider_lines: Vec<Line> = (0..area.height)
        .map(|_| Line::from(Span::styled("┃", divider_style)))
        .collect();

    f.render_widget(Paragraph::new(divider_lines), area);
}

fn draw_right(f: &mut Frame, app: &mut App, area: Rect) {
    let run_idxs = app.runs_for_selected();

    if run_idxs.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "(ещё не запускалось)",
                Style::default().fg(Color::DarkGray),
            )),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = run_idxs
        .iter()
        .map(|&run_id| {
            let run = &app.runs[run_id];

            let sym = if run.ok { "✓" } else { "✗" };
            let color = if run.ok { Color::Green } else { Color::Red };

            let started: DateTime<Local> = run.started_at.into();
            let hhmmss = started.format("%H:%M:%S").to_string();

            let stdout_lines = run.stdout.lines().count();
            let plural = plural_ru(stdout_lines);

            ListItem::new(Line::from(vec![
                Span::styled(
                    sym,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(hhmmss, Style::default().fg(Color::Gray)),
                Span::raw(" "),
                Span::styled(
                    format!("{stdout_lines} {plural}"),
                    Style::default().fg(Color::Gray),
                ),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(highlight_style_for(Panel::Right, app.focus))
        .highlight_symbol(" ");

    f.render_stateful_widget(list, area, &mut app.right_state);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let focus_marker = match app.focus {
        Panel::Left => "[LEFT]",
        Panel::Right => "[RIGHT]",
    };
    let base = if app.status.is_empty() {
        "↑/↓ — навигация, Enter/Space/l — запустить, Tab — фокус, q — выход".to_string()
    } else {
        app.status.clone()
    };
    let line = format!("{focus_marker} {base}");
    let status = Paragraph::new(line).style(Style::default().fg(Color::Gray));
    f.render_widget(status, area);
}

/// «1 строка», «2 строки», «5 строк» — для аккуратной подписи.
fn plural_ru(n: usize) -> &'static str {
    let n10 = n % 10;
    let n100 = n % 100;
    if n10 == 1 && n100 != 11 {
        "строка"
    } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
        "строки"
    } else {
        "строк"
    }
}
