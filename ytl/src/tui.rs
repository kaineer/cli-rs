//! Список заголовков из библиотеки. Поиск открывается по `/`.

use std::io::{self, stdout};

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Terminal;

use crate::fuzzy;
use crate::library::{self, Entry};
use crate::play;

struct App {
    entries: Vec<Entry>,
    query: String,
    /// Строка поиска видна после `/`.
    searching: bool,
    /// Индексы в `entries` после фильтра.
    filtered: Vec<usize>,
    /// Позиция в `filtered`.
    selected: usize,
    status: String,
    debug: bool,
    quit: bool,
}

impl App {
    fn new(entries: Vec<Entry>, debug: bool) -> Self {
        let mut app = Self {
            entries,
            query: String::new(),
            searching: false,
            filtered: Vec::new(),
            selected: 0,
            status: String::new(),
            debug,
            quit: false,
        };
        app.refilter();
        app
    }

    fn titles(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.meta.title.clone()).collect()
    }

    fn refilter(&mut self) {
        let titles = self.titles();
        let query = if self.searching {
            self.query.as_str()
        } else {
            ""
        };
        self.filtered = fuzzy::filter(&titles, query);
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }

    fn current(&self) -> Option<&Entry> {
        self.filtered
            .get(self.selected)
            .and_then(|i| self.entries.get(*i))
    }

    fn move_sel(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            return;
        }
        let len = self.filtered.len() as isize;
        let next = (self.selected as isize + delta).rem_euclid(len);
        self.selected = next as usize;
    }

    fn open_search(&mut self) {
        self.searching = true;
        self.selected = 0;
        self.refilter();
    }

    fn close_search(&mut self) {
        self.searching = false;
        self.query.clear();
        self.refilter();
    }

    fn push_query(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
        self.refilter();
    }

    fn set_status(&mut self, msg: impl Into<String>) {
        if self.debug {
            self.status = msg.into();
        }
    }
}

pub fn run(debug: bool) -> Result<()> {
    let dir = library::music_dir()?;
    let entries = library::list_entries(&dir)?;
    if entries.is_empty() {
        eprintln!("библиотека пуста: нет *.mp3.meta в {}", dir.display());
        eprintln!("скачайте что-нибудь: ytl <youtube-url>");
        return Ok(());
    }

    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(entries, debug);
    let result = loop_ui(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn loop_ui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| draw(f, app))?;

        let Event::Key(key) = event::read().context("чтение клавиатуры")? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Esc if app.searching => app.close_search(),
            KeyCode::Esc => app.quit = true,
            KeyCode::Char('q') if key.modifiers == KeyModifiers::NONE && !app.searching => {
                app.quit = true;
            }
            KeyCode::Enter => play_selected(terminal, app)?,
            KeyCode::Char('x')
                if key.modifiers == KeyModifiers::NONE && !app.searching =>
            {
                delete_selected(app)?;
            }
            KeyCode::Char('/') if key.modifiers == KeyModifiers::NONE && !app.searching => {
                app.open_search();
            }
            KeyCode::Down => app.move_sel(1),
            KeyCode::Up => app.move_sel(-1),
            KeyCode::Char('j')
                if key.modifiers == KeyModifiers::NONE && !app.searching =>
            {
                app.move_sel(1);
            }
            KeyCode::Char('k')
                if key.modifiers == KeyModifiers::NONE && !app.searching =>
            {
                app.move_sel(-1);
            }
            KeyCode::Backspace if app.searching => {
                app.query.pop();
                app.refilter();
            }
            KeyCode::Char(c)
                if app.searching
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                app.push_query(c);
            }
            _ => {}
        }

        if app.quit {
            break;
        }
    }
    Ok(())
}

fn play_selected(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let Some(entry) = app.current().cloned() else {
        app.set_status("нечего играть");
        return Ok(());
    };
    if !entry.mp3.is_file() {
        app.set_status(format!("нет файла {}", entry.mp3.display()));
        return Ok(());
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let play_res = play::play_mp3(&entry.mp3);

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;

    match play_res {
        Ok(()) => app.set_status(format!("ok: {}", entry.meta.title)),
        Err(err) => app.set_status(format!("ошибка: {err:#}")),
    }
    Ok(())
}

fn delete_selected(app: &mut App) -> Result<()> {
    let Some(idx_in_entries) = app.filtered.get(app.selected).copied() else {
        app.set_status("нечего удалять");
        return Ok(());
    };
    let entry = app.entries[idx_in_entries].clone();
    match library::delete_entry(&entry) {
        Ok(()) => {
            app.entries.remove(idx_in_entries);
            app.set_status(format!("удалено: {}", entry.meta.title));
            app.refilter();
        }
        Err(err) => {
            app.set_status(format!("не удалось удалить: {err:#}"));
        }
    }
    Ok(())
}

fn draw(f: &mut ratatui::Frame, app: &App) {
    let area = f.area();

    let show_search = app.searching;
    let show_chrome = app.debug;

    let mut constraints = Vec::new();
    if show_search {
        constraints.push(Constraint::Length(if show_chrome { 3 } else { 1 }));
    }
    constraints.push(Constraint::Min(1));
    if show_chrome {
        constraints.push(Constraint::Length(1));
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;

    if show_search {
        let search_line = Line::from(vec![
            Span::styled("/ ", Style::default().fg(Color::Cyan)),
            Span::raw(&app.query),
        ]);
        let search = if show_chrome {
            Paragraph::new(search_line).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" поиск ")
                    .border_style(Style::default().fg(Color::Rgb(0x4c, 0x56, 0x6a))),
            )
        } else {
            Paragraph::new(search_line)
        };
        f.render_widget(search, chunks[idx]);
        idx += 1;
    }

    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .enumerate()
        .map(|(i, &entry_idx)| {
            let entry = &app.entries[entry_idx];
            let missing = if entry.mp3.is_file() {
                ""
            } else {
                " [нет mp3]"
            };
            let line = format!("{}{missing}", entry.meta.title);
            let style = if i == app.selected {
                Style::default()
                    .bg(Color::Rgb(0x5e, 0x81, 0xac))
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(0xd8, 0xde, 0xe9))
            };
            ListItem::new(Line::from(Span::styled(line, style)))
        })
        .collect();

    let list = if show_chrome {
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" библиотека ({}) ", app.filtered.len()))
                .border_style(Style::default().fg(Color::Rgb(0x4c, 0x56, 0x6a))),
        )
    } else {
        List::new(items)
    };
    f.render_widget(list, chunks[idx]);
    idx += 1;

    if show_chrome {
        let hint = if app.status.is_empty() {
            "enter: play  x: delete  /: search  q/esc: quit".to_string()
        } else {
            app.status.clone()
        };
        f.render_widget(
            Paragraph::new(hint).style(Style::default().fg(Color::Rgb(0x88, 0xc0, 0xd0))),
            chunks[idx],
        );
    }
}
