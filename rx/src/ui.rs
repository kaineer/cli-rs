use chrono::{DateTime, Local};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
    Frame,
};

use crate::app::{format_duration, App, EntryState, Panel, RunView, PREVIEW_LINES};
use crate::run_record::RunRecord;

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

    app.left_panel_rect = horizontal[0];
    app.right_panel_rect = horizontal[2];

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
    let divider_style = if app.dragging {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        match app.focus {
            Panel::Left => Style::default().fg(Color::Rgb(0x5e, 0x81, 0xac)),
            Panel::Right => Style::default().fg(Color::Rgb(0xbf, 0x61, 0x6a)), // Nord red
        }
    };
    let divider_lines: Vec<Line> = (0..area.height)
        .map(|_| Line::from(Span::styled("┃", divider_style)))
        .collect();
    f.render_widget(Paragraph::new(divider_lines), area);
}

/// Бейдж «необычного» завершения: KILL/SIGn/exit N/ERR.
/// Для обычных 0 и 1 ничего не возвращаем.
fn failure_badge(run: &RunRecord) -> Option<Span<'static>> {
    if run.ok {
        return None;
    }
    let red = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);

    if let Some(sig) = run.signal {
        let text = match sig {
            9 => "KILL".to_string(),
            n => format!("SIG{n}"),
        };
        return Some(Span::styled(text, red));
    }
    if let Some(code) = run.exit_code {
        if code == 1 {
            return None;
        }
        return Some(Span::styled(format!("exit {code}"), red));
    }
    // Ни кода, ни сигнала — обычно ошибка spawn/wait.
    Some(Span::styled("ERR", red))
}

fn draw_right(f: &mut Frame, app: &mut App, area: Rect) {
    app.right_area_height = area.height;

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

    app.recompute_right_first();

    let sel = app
        .right_state
        .selected()
        .unwrap_or(0)
        .min(run_idxs.len() - 1);

    let in_body = app.in_body_mode();
    let focus_style = highlight_style_for(Panel::Right, app.focus);
    let mut lines: Vec<Line> = Vec::new();
    let max_rows = area.height as usize;

    let first = app.right_first_visible.min(run_idxs.len() - 1);

    for (offset, &run_id) in run_idxs.iter().enumerate().skip(first) {
        let run = &app.runs[run_id];
        let is_selected = offset == sel;

        // В Body всё, кроме выделенного, схлопываем.
        let effective_view = if in_body && !is_selected {
            RunView::Collapsed
        } else {
            app.run_views[run_id].view
        };

        let subdued = in_body && !is_selected;

        let sym = if run.ok { "✓" } else { "✗" };
        let sym_color = if subdued {
            Color::DarkGray
        } else if run.ok {
            Color::Green
        } else {
            Color::Red
        };
        let meta_color = if subdued {
            Color::DarkGray
        } else {
            Color::Gray
        };

        let started: DateTime<Local> = run.started_at.into();
        let hhmmss = started.format("%H:%M:%S").to_string();
        let total = run.stdout.lines().count();
        let plural = plural_ru(total);

        let mut header_spans = vec![
            Span::styled(
                sym,
                Style::default().fg(sym_color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(hhmmss, Style::default().fg(meta_color)),
            Span::raw(" "),
        ];
        if let Some(badge) = failure_badge(run) {
            header_spans.push(badge);
            header_spans.push(Span::raw(" "));
        }
        header_spans.push(Span::styled(
            format!("{total} {plural}"),
            Style::default().fg(meta_color),
        ));

        let header_line = if is_selected && app.focus == Panel::Right {
            let mut spans = Vec::with_capacity(header_spans.len() + 1);
            spans.push(Span::styled(" ", focus_style));
            for s in header_spans {
                spans.push(Span::styled(s.content.into_owned(), focus_style));
            }
            Line::from(spans)
        } else {
            let mut spans = Vec::with_capacity(header_spans.len() + 1);
            spans.push(Span::raw(" "));
            spans.extend(header_spans);
            Line::from(spans)
        };
        lines.push(header_line);

        if lines.len() >= max_rows {
            break;
        }

        let body_rows = match effective_view {
            RunView::Collapsed => 0,
            RunView::Preview => total.min(PREVIEW_LINES),
            RunView::Full => total,
        };

        if body_rows > 0 {
            let vs = app.run_views[run_id];
            let all: Vec<&str> = run.stdout.lines().collect();
            let start = vs.scroll.min(all.len());
            let end = (start + body_rows).min(all.len());
            for l in &all[start..end] {
                lines.push(Line::from(format!("  {l}")));
                if lines.len() >= max_rows {
                    break;
                }
            }

            if effective_view == RunView::Preview && lines.len() < max_rows {
                let shown = body_rows;
                let rest = total.saturating_sub(shown);
                if rest > 0 {
                    lines.push(Line::from(Span::styled(
                        format!("--- ещё {rest} {} ---", plural_ru(rest)),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
        }

        if lines.len() >= max_rows {
            break;
        }
    }

    lines.truncate(max_rows);

    f.render_widget(Paragraph::new(lines), area);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let focus_marker = match app.focus {
        Panel::Left => "[LEFT]",
        Panel::Right => "[RIGHT]",
    };
    let base = if app.status.is_empty() {
        "↑/↓ — навигация, Space — раскрыть, Tab — фокус, q — выход".to_string()
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
