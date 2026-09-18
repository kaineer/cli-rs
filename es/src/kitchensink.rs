//! Interactive demo of all form widgets.

use std::io::{self, stdout};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::widgets::{
    Checkbox, Enable, Password, Select, SelectOption, Text, TextArea,
};

enum Field {
    Text(Text),
    Password(Password),
    TextArea(TextArea),
    Select(Select),
    Checkbox(Checkbox),
    Enable(Enable),
}

impl Field {
    fn height(&self) -> u16 {
        match self {
            Field::Text(w) => w.height(),
            Field::Password(w) => w.height(),
            Field::TextArea(w) => w.height(),
            Field::Select(w) => w.height(),
            Field::Checkbox(w) => w.height(),
            Field::Enable(w) => w.height(),
        }
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        match self {
            Field::Text(w) => w.handle_key(key),
            Field::Password(w) => w.handle_key(key),
            Field::TextArea(w) => w.handle_key(key),
            Field::Select(w) => w.handle_key(key),
            Field::Checkbox(w) => w.handle_key(key),
            Field::Enable(w) => w.handle_key(key),
        }
    }

    fn render(&mut self, f: &mut ratatui::Frame, area: Rect, focused: bool) {
        match self {
            Field::Text(w) => w.render(f, area, focused),
            Field::Password(w) => w.render(f, area, focused),
            Field::TextArea(w) => w.render(f, area, focused),
            Field::Select(w) => w.render(f, area, focused),
            Field::Checkbox(w) => w.render(f, area, focused),
            Field::Enable(w) => w.render(f, area, focused),
        }
    }

    fn select_open(&self) -> bool {
        matches!(self, Field::Select(s) if s.open)
    }

    fn summary(&self) -> String {
        match self {
            Field::Text(w) => format!("text={:?}", w.value),
            Field::Password(w) => format!("password=<{} chars>", w.value.chars().count()),
            Field::TextArea(w) => {
                format!("textarea={:?}", w.text().replace('\n', "\\n"))
            }
            Field::Select(w) => format!("select={:?}", w.value().unwrap_or("")),
            Field::Checkbox(w) => format!("checkbox={:?}", w.value),
            Field::Enable(w) => format!("enable={:?}", w.value),
        }
    }
}

/// Y-offset of each field and total content height (including gaps).
fn field_layout(fields: &[Field]) -> (Vec<u16>, u16) {
    let mut offsets = Vec::with_capacity(fields.len());
    let mut y = 0u16;
    for (i, field) in fields.iter().enumerate() {
        offsets.push(y);
        y = y.saturating_add(field.height());
        if i + 1 < fields.len() {
            y = y.saturating_add(1);
        }
    }
    (offsets, y)
}

struct App {
    fields: Vec<Field>,
    focus: usize,
    /// First visible content row.
    scroll: u16,
    should_quit: bool,
}

impl App {
    fn new() -> Self {
        Self {
            fields: vec![
                Field::Text(Text::new("text", "John")),
                Field::Password(Password::new("password", "secret")),
                Field::TextArea(TextArea::new(
                    "textarea",
                    "Короткая строка.\nА эта строка специально длинная, чтобы было видно soft-wrap с маркером ↩ справа на перенесённых рядах.",
                    5,
                )),
                Field::Select(Select::new(
                    "select",
                    vec![
                        SelectOption {
                            label: "Red".into(),
                            value: "1".into(),
                        },
                        SelectOption {
                            label: "Green".into(),
                            value: "2".into(),
                        },
                        SelectOption {
                            label: "Blue".into(),
                            value: "3".into(),
                        },
                    ],
                    0,
                )),
                Field::Checkbox(Checkbox::new("checkbox", Some(false))),
                Field::Enable(Enable::new("enable", Some(true), "enabled", "disabled")),
                Field::Enable(Enable::new("enable (ru)", None, "вкл", "выкл")),
                // Extra fields so scroll is easy to notice in a tall terminal.
                Field::Text(Text::new("extra text", "")),
                Field::Checkbox(Checkbox::new("extra checkbox", None)),
                Field::Enable(Enable::new("extra enable", Some(false), "on", "off")),
            ],
            focus: 0,
            scroll: 0,
            should_quit: false,
        }
    }

    fn focus_next(&mut self) {
        if self.fields[self.focus].select_open() {
            return;
        }
        self.focus = (self.focus + 1) % self.fields.len();
    }

    fn focus_prev(&mut self) {
        if self.fields[self.focus].select_open() {
            return;
        }
        self.focus = if self.focus == 0 {
            self.fields.len() - 1
        } else {
            self.focus - 1
        };
    }

    /// Keep the focused field fully visible inside `viewport_h`.
    fn ensure_focus_visible(&mut self, offsets: &[u16], viewport_h: u16, total_h: u16) {
        if viewport_h == 0 || self.focus >= self.fields.len() {
            return;
        }
        let top = offsets[self.focus];
        let bottom = top.saturating_add(self.fields[self.focus].height());

        if top < self.scroll {
            self.scroll = top;
        }
        if bottom > self.scroll.saturating_add(viewport_h) {
            self.scroll = bottom.saturating_sub(viewport_h);
        }

        let max_scroll = total_h.saturating_sub(viewport_h);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    fn scroll_by(&mut self, delta: i32, viewport_h: u16, total_h: u16) {
        let max_scroll = total_h.saturating_sub(viewport_h) as i32;
        let next = (self.scroll as i32 + delta).clamp(0, max_scroll);
        self.scroll = next as u16;
    }
}

pub fn run() -> Result<()> {
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alt screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    let mut app = App::new();
    let result = loop_ui(&mut terminal, &mut app);

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    result
}

fn loop_ui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|f| draw(f, app))?;
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let size = terminal.size()?;
                // Approximate content viewport: full height minus header/values/status/borders.
                let viewport_h = size.height.saturating_sub(1 + 3 + 1 + 2);
                let (_, total_h) = field_layout(&app.fields);
                handle_key(app, key, viewport_h, total_h);
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: crossterm::event::KeyEvent, viewport_h: u16, total_h: u16) {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), KeyModifiers::CONTROL)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return;
        }
        (KeyCode::Esc, _) if !app.fields[app.focus].select_open() => {
            app.should_quit = true;
            return;
        }
        (KeyCode::Tab, _) => {
            app.focus_next();
            return;
        }
        (KeyCode::BackTab, _) => {
            app.focus_prev();
            return;
        }
        (KeyCode::PageDown, _) => {
            app.scroll_by(viewport_h as i32, viewport_h, total_h);
            return;
        }
        (KeyCode::PageUp, _) => {
            app.scroll_by(-(viewport_h as i32), viewport_h, total_h);
            return;
        }
        _ => {}
    }

    let _ = app.fields[app.focus].handle_key(key);
}

fn draw(f: &mut ratatui::Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " kitchensink — Tab/S-Tab focus · PgUp/PgDn scroll · Esc/C-q quit",
            Style::default().fg(Color::Cyan),
        ))),
        chunks[0],
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" widgets ");
    let inner = block.inner(chunks[1]);
    f.render_widget(block, chunks[1]);

    let (offsets, total_h) = field_layout(&app.fields);
    let needs_scroll = total_h > inner.height;

    let (content_area, scrollbar_area) = if needs_scroll && inner.width > 1 {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);
        (split[0], Some(split[1]))
    } else {
        (inner, None)
    };

    app.ensure_focus_visible(&offsets, content_area.height, total_h);

    let scroll = app.scroll;
    let view_bottom = scroll.saturating_add(content_area.height);

    for (i, field) in app.fields.iter_mut().enumerate() {
        let top = offsets[i];
        let h = field.height();
        let bottom = top.saturating_add(h);

        // Fully visible fields only (focus is kept fully in view).
        if top < scroll || bottom > view_bottom {
            continue;
        }

        let rect = Rect {
            x: content_area.x,
            y: content_area.y + (top - scroll),
            width: content_area.width,
            height: h,
        };
        field.render(f, rect, i == app.focus);
    }

    if let Some(sb_area) = scrollbar_area {
        let max_scroll = total_h.saturating_sub(content_area.height) as usize;
        let mut state = ScrollbarState::new(max_scroll).position(scroll as usize);
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"))
                .track_symbol(Some("│"))
                .thumb_symbol("█"),
            sb_area,
            &mut state,
        );
    }

    let values: Vec<String> = app.fields.iter().map(|f| f.summary()).collect();
    f.render_widget(
        Paragraph::new(values.join("  ·  "))
            .block(Block::default().borders(Borders::ALL).title(" values ")),
        chunks[2],
    );

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(
                " focus: {}/{}  scroll: {}  content: {}",
                app.focus,
                app.fields.len().saturating_sub(1),
                scroll,
                total_h
            ),
            Style::default().fg(Color::DarkGray),
        ))),
        chunks[3],
    );
}
