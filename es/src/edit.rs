//! `es edit` — TUI form editor driven by a schema.

use std::fs;
use std::io::{self, stdout};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};
use serde_yaml::{Mapping, Value};

use crate::io_args::IoArgs;
use crate::parse::{self, Field as SchemaField, Schema, ValueType, Widget as SchemaWidget};
use crate::widgets::{
    Button, Checkbox, Enable, Password, Select, SelectOption, Text, TextArea,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Field(usize),
    Save,
}

enum FormWidget {
    Text { w: Text, defined: bool },
    Password { w: Password, defined: bool },
    TextArea { w: TextArea, defined: bool },
    Select(Select),
    Checkbox(Checkbox),
    Enable(Enable),
}

struct FormField {
    key: String,
    value_type: ValueType,
    widget: FormWidget,
}

impl FormField {
    fn height(&self) -> u16 {
        match &self.widget {
            FormWidget::Text { w, .. } => w.height(),
            FormWidget::Password { w, .. } => w.height(),
            FormWidget::TextArea { w, .. } => w.height(),
            FormWidget::Select(w) => w.height(),
            FormWidget::Checkbox(w) => w.height(),
            FormWidget::Enable(w) => w.height(),
        }
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        match &mut self.widget {
            FormWidget::Text { w, defined } => {
                let changed = w.handle_key(key);
                if changed {
                    *defined = true;
                }
                changed
            }
            FormWidget::Password { w, defined } => {
                let changed = w.handle_key(key);
                if changed {
                    *defined = true;
                }
                changed
            }
            FormWidget::TextArea { w, defined } => {
                let changed = w.handle_key(key);
                if changed {
                    *defined = true;
                }
                changed
            }
            FormWidget::Select(w) => w.handle_key(key),
            FormWidget::Checkbox(w) => w.handle_key(key),
            FormWidget::Enable(w) => w.handle_key(key),
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, focused: bool) {
        match &mut self.widget {
            FormWidget::Text { w, .. } => w.render(f, area, focused),
            FormWidget::Password { w, .. } => w.render(f, area, focused),
            FormWidget::TextArea { w, .. } => w.render(f, area, focused),
            FormWidget::Select(w) => w.render(f, area, focused),
            FormWidget::Checkbox(w) => w.render(f, area, focused),
            FormWidget::Enable(w) => w.render(f, area, focused),
        }
    }

    fn is_editing(&self) -> bool {
        match &self.widget {
            FormWidget::Text { w, .. } => w.is_editing(),
            FormWidget::Password { w, .. } => w.is_editing(),
            FormWidget::TextArea { w, .. } => w.is_editing(),
            FormWidget::Select(s) => s.open(),
            _ => false,
        }
    }

    fn end_editing(&mut self) {
        match &mut self.widget {
            FormWidget::Text { w, .. } => w.editing = false,
            FormWidget::Password { w, .. } => w.editing = false,
            FormWidget::TextArea { w, .. } => w.editing = false,
            _ => {}
        }
    }

    fn to_yaml(&self) -> Option<Value> {
        match (&self.widget, &self.value_type) {
            (FormWidget::Text { w, defined }, vt) if *defined => Some(scalar_string(&w.value, vt)),
            (FormWidget::Password { w, defined }, vt) if *defined => {
                Some(scalar_string(&w.value, vt))
            }
            (FormWidget::TextArea { w, defined }, vt) if *defined => {
                Some(scalar_string(&w.text(), vt))
            }
            (FormWidget::Select(w), vt) => w.value().map(|v| scalar_string(v, vt)),
            (FormWidget::Checkbox(w), _) => w.value.map(Value::Bool),
            (FormWidget::Enable(w), _) => w.value.map(Value::Bool),
            _ => None,
        }
    }
}

fn scalar_string(s: &str, vt: &ValueType) -> Value {
    match vt {
        ValueType::Boolean => match s {
            "true" | "True" | "yes" | "1" => Value::Bool(true),
            "false" | "False" | "no" | "0" => Value::Bool(false),
            _ => Value::String(s.to_string()),
        },
        ValueType::Number => {
            if let Ok(i) = s.parse::<i64>() {
                Value::Number(i.into())
            } else if let Ok(f) = s.parse::<f64>() {
                Value::Number(serde_yaml::Number::from(f))
            } else {
                Value::String(s.to_string())
            }
        }
        _ => Value::String(s.to_string()),
    }
}

fn field_layout(fields: &[FormField]) -> (Vec<u16>, u16) {
    let mut offsets = Vec::with_capacity(fields.len());
    let mut y = 0u16;
    for field in fields {
        offsets.push(y);
        y = y.saturating_add(field.height());
    }
    (offsets, y)
}

struct App {
    fields: Vec<FormField>,
    focus: Focus,
    scroll: u16,
    save_btn: Button,
    output: PathBuf,
    should_quit: bool,
    saved: bool,
    status: String,
}

impl App {
    fn from_schema(schema: &Schema, data: &Mapping, output: PathBuf) -> Result<Self> {
        if schema.fields.is_empty() {
            bail!("в схеме нет полей для редактирования");
        }
        let fields: Vec<FormField> = schema
            .fields
            .iter()
            .map(|f| build_field(f, data))
            .collect::<Result<_>>()?;
        Ok(Self {
            fields,
            focus: Focus::Field(0),
            scroll: 0,
            save_btn: Button::new("Save"),
            output,
            should_quit: false,
            saved: false,
            status: String::new(),
        })
    }

    fn focus_next(&mut self) {
        if let Focus::Field(i) = self.focus {
            self.fields[i].end_editing();
        }
        self.focus = match self.focus {
            Focus::Field(i) if i + 1 < self.fields.len() => Focus::Field(i + 1),
            Focus::Field(_) => Focus::Save,
            Focus::Save => Focus::Field(0),
        };
    }

    fn focus_prev(&mut self) {
        if let Focus::Field(i) = self.focus {
            self.fields[i].end_editing();
        }
        self.focus = match self.focus {
            Focus::Field(0) => Focus::Save,
            Focus::Field(i) => Focus::Field(i - 1),
            Focus::Save => Focus::Field(self.fields.len() - 1),
        };
    }

    fn ensure_focus_visible(&mut self, offsets: &[u16], viewport_h: u16, total_h: u16) {
        let Focus::Field(i) = self.focus else {
            return;
        };
        if viewport_h == 0 || i >= self.fields.len() {
            return;
        }
        let top = offsets[i];
        let bottom = top.saturating_add(self.fields[i].height());
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

    fn save(&mut self) -> Result<()> {
        let mut map = Mapping::new();
        for field in &self.fields {
            if let Some(v) = field.to_yaml() {
                map.insert(Value::String(field.key.clone()), v);
            }
        }
        let text = serde_yaml::to_string(&Value::Mapping(map)).context("сериализация YAML")?;
        if let Some(parent) = self.output.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("не удалось создать {}", parent.display()))?;
            }
        }
        fs::write(&self.output, text)
            .with_context(|| format!("не удалось записать {}", self.output.display()))?;
        self.saved = true;
        self.should_quit = true;
        Ok(())
    }
}

fn build_field(schema: &SchemaField, data: &Mapping) -> Result<FormField> {
    let label = schema
        .label
        .clone()
        .unwrap_or_else(|| schema.key.clone());
    let key_v = Value::String(schema.key.clone());
    let raw = data.get(&key_v);
    let in_data = raw.is_some();

    let widget = match &schema.widget {
        SchemaWidget::Text => {
            let (value, defined) = string_value(raw, &schema.default, in_data);
            FormWidget::Text {
                w: Text::new(label, value),
                defined,
            }
        }
        SchemaWidget::Password { empty, filled } => {
            let (value, defined) = string_value(raw, &schema.default, in_data);
            FormWidget::Password {
                w: Password::with_hints(label, value, empty.clone(), filled.clone()),
                defined,
            }
        }
        SchemaWidget::Textarea { height } => {
            let (value, defined) = string_value(raw, &schema.default, in_data);
            let h = height.unwrap_or(4).max(1) as u16;
            FormWidget::TextArea {
                w: TextArea::new(label, value, h),
                defined,
            }
        }
        SchemaWidget::Select { options } => {
            let opts: Vec<SelectOption> = options
                .iter()
                .map(|o| SelectOption {
                    label: o.label.clone(),
                    value: o.value.clone(),
                })
                .collect();
            let mut sel = Select::new(label, opts, 0);
            if let Some(v) = raw.and_then(value_as_string).or_else(|| schema.default.clone()) {
                if let Some(idx) = sel.options.iter().position(|o| o.value == v || o.label == v)
                {
                    sel.selected = idx;
                    sel.set = true;
                } else {
                    sel = sel.unset();
                }
            } else {
                sel = sel.unset();
            }
            FormWidget::Select(sel)
        }
        SchemaWidget::Checkbox => {
            FormWidget::Checkbox(Checkbox::new(label, bool_value(raw, &schema.default)))
        }
        SchemaWidget::Enable { on, off } => {
            let on = on.clone().unwrap_or_else(|| "enabled".into());
            let off = off.clone().unwrap_or_else(|| "disabled".into());
            FormWidget::Enable(Enable::new(
                label,
                bool_value(raw, &schema.default),
                on,
                off,
            ))
        }
        SchemaWidget::Other(name) => bail!("неподдерживаемый виджет: {name}"),
    };

    Ok(FormField {
        key: schema.key.clone(),
        value_type: schema.value_type.clone(),
        widget,
    })
}

fn string_value(raw: Option<&Value>, default: &Option<String>, in_data: bool) -> (String, bool) {
    if let Some(v) = raw.and_then(value_as_string) {
        return (v, true);
    }
    if let Some(d) = default {
        return (d.clone(), true);
    }
    (String::new(), in_data)
}

fn bool_value(raw: Option<&Value>, default: &Option<String>) -> Option<bool> {
    if let Some(Value::Bool(b)) = raw {
        return Some(*b);
    }
    if let Some(s) = raw.and_then(value_as_string) {
        return parse_bool_str(&s);
    }
    default.as_deref().and_then(parse_bool_str)
}

fn parse_bool_str(s: &str) -> Option<bool> {
    match s {
        "true" | "True" | "yes" | "1" => Some(true),
        "false" | "False" | "no" | "0" => Some(false),
        _ => None,
    }
}

fn value_as_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn load_input(path: &Path) -> Result<Mapping> {
    if !path.exists() {
        return Ok(Mapping::new());
    }
    if !path.is_file() {
        bail!("input не файл: {}", path.display());
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("не удалось прочитать {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(Mapping::new());
    }
    let root: Value = serde_yaml::from_str(&text)
        .with_context(|| format!("невалидный YAML в {}", path.display()))?;
    match root {
        Value::Mapping(m) => Ok(m),
        Value::Null => Ok(Mapping::new()),
        _ => bail!("корень input должен быть YAML-мапой"),
    }
}

pub fn run(args: IoArgs) -> Result<()> {
    if !args.scheme.is_file() {
        bail!("схема не найдена: {}", args.scheme.display());
    }
    let schema = parse::parse_file(&args.scheme)?;
    let data = load_input(&args.input)?;

    enable_raw_mode().context("enable raw mode")?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen).context("enter alt screen")?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    let mut app = App::from_schema(&schema, &data, args.output.clone())?;
    let result = loop_ui(&mut terminal, &mut app);

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result?;
    if app.saved {
        eprintln!("saved: {}", args.output.display());
    } else {
        eprintln!("cancelled — файл не изменён");
    }
    Ok(())
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
                let viewport_h = size.height.saturating_sub(1 + 1 + 1);
                let (_, total_h) = field_layout(&app.fields);
                if let Err(e) = handle_key(app, key, viewport_h, total_h) {
                    app.status = format!("ошибка: {e:#}");
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn handle_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    viewport_h: u16,
    total_h: u16,
) -> Result<()> {
    let editing = matches!(app.focus, Focus::Field(i) if app.fields[i].is_editing());

    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), KeyModifiers::CONTROL)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return Ok(());
        }
        (KeyCode::Esc, _) if !editing => {
            app.should_quit = true;
            return Ok(());
        }
        (KeyCode::Tab, _) => {
            app.focus_next();
            return Ok(());
        }
        (KeyCode::BackTab, _) => {
            app.focus_prev();
            return Ok(());
        }
        (KeyCode::PageDown, _) => {
            app.scroll_by(viewport_h as i32, viewport_h, total_h);
            return Ok(());
        }
        (KeyCode::PageUp, _) => {
            app.scroll_by(-(viewport_h as i32), viewport_h, total_h);
            return Ok(());
        }
        _ => {}
    }

    match app.focus {
        Focus::Field(i) => {
            let _ = app.fields[i].handle_key(key);
        }
        Focus::Save => {
            if app.save_btn.handle_key(key) {
                app.save()?;
            }
        }
    }
    Ok(())
}

fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " edit — Enter edit · Tab leave/next · Esc discard · Space Save",
            Style::default().fg(Color::Cyan),
        ))),
        chunks[0],
    );

    let form_area = chunks[1];
    let (offsets, total_h) = field_layout(&app.fields);
    let needs_scroll = total_h > form_area.height;
    let (content_area, scrollbar_area) = if needs_scroll && form_area.width > 1 {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(form_area);
        (split[0], Some(split[1]))
    } else {
        (form_area, None)
    };

    app.ensure_focus_visible(&offsets, content_area.height, total_h);
    let scroll = app.scroll;
    let view_bottom = scroll.saturating_add(content_area.height);

    for (i, field) in app.fields.iter_mut().enumerate() {
        let top = offsets[i];
        let h = field.height();
        let bottom = top.saturating_add(h);
        if top < scroll || bottom > view_bottom {
            continue;
        }
        let rect = Rect {
            x: content_area.x,
            y: content_area.y + (top - scroll),
            width: content_area.width,
            height: h,
        };
        field.render(f, rect, app.focus == Focus::Field(i));
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

    let btn_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(10), Constraint::Min(0)])
        .split(chunks[2]);
    app.save_btn
        .render(f, btn_row[0], app.focus == Focus::Save);

    let status = if app.status.is_empty() {
        format!(" output: {}", app.output.display())
    } else {
        app.status.clone()
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {status}"),
            Style::default().fg(Color::DarkGray),
        ))),
        chunks[3],
    );
}
