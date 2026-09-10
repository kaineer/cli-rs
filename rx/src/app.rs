use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use ratatui::widgets::ListState;

use crate::config::MenuEntry;
use crate::run_record::RunRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Left,
    Right,
}

/// Рантайм-состояние одной записи списка.
pub enum EntryState {
    Idle,
    Running,
    /// run_id — индекс в `App::runs`.
    Done { run_id: usize },
}

pub struct App {
    pub entries: Vec<MenuEntry>,
    pub states: Vec<EntryState>,
    pub list_state: ListState,
    pub right_state: ListState,
    pub focus: Panel,
    pub left_width: u16,
    pub dragging: bool,
    pub area_width: u16,
    pub status: String,
    pub should_quit: bool,
    pub tick: u64,

    /// История всех запусков в порядке появления.
    pub runs: Vec<RunRecord>,

    /// Активный процесс (только один одновременно).
    running: Option<RunningProcess>,
}

struct RunningProcess {
    entry_idx: usize,
    child: Child,
    started_at: SystemTime,
    started_instant: Instant,
    rx: Receiver<StreamLine>,
    stdout_buf: String,
    stderr_buf: String,
}

enum StreamLine {
    Stdout(String),
    Stderr(String),
    Eof,
}

impl App {
    pub fn new(entries: Vec<MenuEntry>) -> Self {
        let states = entries.iter().map(|_| EntryState::Idle).collect();
        let mut list_state = ListState::default();
        if !entries.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            entries,
            states,
            list_state,
            right_state: ListState::default(),
            focus: Panel::Left,
            left_width: 12,
            dragging: false,
            area_width: 80,
            status: String::new(),
            should_quit: false,
            tick: 0,
            runs: Vec::new(),
            running: None,
        }
    }

    pub fn next(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => (i + 1) % self.entries.len(),
            None => 0,
        };
        self.list_state.select(Some(i));
        self.on_left_selection_changed();
    }

    pub fn previous(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let len = self.entries.len();
        let i = match self.list_state.selected() {
            Some(0) => len - 1,
            Some(i) => i - 1,
            None => 0,
        };
        self.list_state.select(Some(i));
        self.on_left_selection_changed();
    }

    pub fn selected_entry(&self) -> Option<&MenuEntry> {
        self.list_state
            .selected()
            .and_then(|i| self.entries.get(i))
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        self.poll_running();
    }

    pub fn activate_selected(&mut self) {
        if let Some(idx) = self.list_state.selected() {
            self.activate(idx);
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Panel::Left => {
                let n = self.runs_for_selected().len();
                if n > 0 && self.right_state.selected().is_none() {
                    self.right_state.select(Some(0));
                }
                Panel::Right
            }
            Panel::Right => Panel::Left,
        };
    }

    /// Индексы в `self.runs` для выбранного элемента, свежие сверху.
    pub fn runs_for_selected(&self) -> Vec<usize> {
        let Some(idx) = self.list_state.selected() else {
            return Vec::new();
        };
        self.runs
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, r)| r.entry_idx == idx)
            .map(|(i, _)| i)
            .collect()
    }

    fn on_left_selection_changed(&mut self) {
        self.right_state = ListState::default();
        let n = self.runs_for_selected().len();
        if n > 0 {
            self.right_state.select(Some(0));
        }
    }

    pub fn right_next(&mut self) {
        let n = self.runs_for_selected().len();
        if n == 0 {
            return;
        }
        let i = match self.right_state.selected() {
            Some(i) => (i + 1) % n,
            None => 0,
        };
        self.right_state.select(Some(i));
    }

    pub fn right_previous(&mut self) {
        let n = self.runs_for_selected().len();
        if n == 0 {
            return;
        }
        let i = match self.right_state.selected() {
            Some(0) => n - 1,
            Some(i) => i - 1,
            None => 0,
        };
        self.right_state.select(Some(i));
    }

    fn activate(&mut self, idx: usize) {
        if self.running.is_some() {
            self.status = "Уже выполняется другая команда".to_string();
            return;
        }

        let cmd = self.entries[idx].cmd.clone();

        let mut cmd_builder = Command::new("sh");
        cmd_builder
            .arg("-c")
            .arg(&cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd_builder.process_group(0);
        }

        let mut child = match cmd_builder.spawn() {
            Ok(c) => c,
            Err(e) => {
                self.status = format!("Ошибка запуска: {e}");
                let now = SystemTime::now();
                let record = RunRecord {
                    entry_idx: idx,
                    started_at: now,
                    finished_at: now,
                    duration: Duration::ZERO,
                    exit_code: None,
                    ok: false,
                    stdout: String::new(),
                    stderr: e.to_string(),
                };
                self.runs.push(record);
                let run_id = self.runs.len() - 1;
                self.states[idx] = EntryState::Done { run_id };
                self.on_left_selection_changed();
                return;
            }
        };

        let (tx, rx) = mpsc::channel::<StreamLine>();

        if let Some(out) = child.stdout.take() {
            let tx = tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(out);
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            let _ = tx.send(StreamLine::Stdout(l));
                        }
                        Err(_) => break,
                    }
                }
                let _ = tx.send(StreamLine::Eof);
            });
        }
        if let Some(err) = child.stderr.take() {
            let tx = tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(err);
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            let _ = tx.send(StreamLine::Stderr(l));
                        }
                        Err(_) => break,
                    }
                }
                let _ = tx.send(StreamLine::Eof);
            });
        }
        drop(tx);

        let started_at = SystemTime::now();
        let started_instant = Instant::now();
        self.states[idx] = EntryState::Running;
        self.running = Some(RunningProcess {
            entry_idx: idx,
            child,
            started_at,
            started_instant,
            rx,
            stdout_buf: String::new(),
            stderr_buf: String::new(),
        });
        self.status = format!("Запущено: {}", self.entries[idx].label);
    }

    fn poll_running(&mut self) {
        let Some(rp) = self.running.as_mut() else {
            return;
        };

        loop {
            match rp.rx.try_recv() {
                Ok(StreamLine::Stdout(l)) => {
                    rp.stdout_buf.push_str(&l);
                    rp.stdout_buf.push('\n');
                }
                Ok(StreamLine::Stderr(l)) => {
                    rp.stderr_buf.push_str(&l);
                    rp.stderr_buf.push('\n');
                }
                Ok(StreamLine::Eof) => {}
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }

        match rp.child.try_wait() {
            Ok(Some(status)) => {
                for _ in 0..5 {
                    match rp.rx.try_recv() {
                        Ok(StreamLine::Stdout(l)) => {
                            rp.stdout_buf.push_str(&l);
                            rp.stdout_buf.push('\n');
                        }
                        Ok(StreamLine::Stderr(l)) => {
                            rp.stderr_buf.push_str(&l);
                            rp.stderr_buf.push('\n');
                        }
                        _ => break,
                    }
                }

                let duration = rp.started_instant.elapsed();
                let exit_code = status.code();
                let ok = exit_code == Some(0);
                let finished_at = SystemTime::now();

                let record = RunRecord {
                    entry_idx: rp.entry_idx,
                    started_at: rp.started_at,
                    finished_at,
                    duration,
                    exit_code,
                    ok,
                    stdout: std::mem::take(&mut rp.stdout_buf),
                    stderr: std::mem::take(&mut rp.stderr_buf),
                };

                let entry_idx = rp.entry_idx;
                self.runs.push(record);
                let run_id = self.runs.len() - 1;
                self.states[entry_idx] = EntryState::Done { run_id };
                self.status = format!(
                    "{} {}",
                    self.entries[entry_idx].label,
                    if ok { "ok" } else { "ошибка" },
                );
                self.running = None;
                // Обновляем курсор истории, если сейчас выбран этот же элемент
                if self.list_state.selected() == Some(entry_idx) {
                    self.on_left_selection_changed();
                }
            }
            Ok(None) => {}
            Err(e) => {
                let duration = rp.started_instant.elapsed();
                let finished_at = SystemTime::now();
                let record = RunRecord {
                    entry_idx: rp.entry_idx,
                    started_at: rp.started_at,
                    finished_at,
                    duration,
                    exit_code: None,
                    ok: false,
                    stdout: std::mem::take(&mut rp.stdout_buf),
                    stderr: format!("wait error: {e}"),
                };
                let entry_idx = rp.entry_idx;
                self.runs.push(record);
                let run_id = self.runs.len() - 1;
                self.states[entry_idx] = EntryState::Done { run_id };
                self.status = format!("Ошибка ожидания процесса: {e}");
                self.running = None;
                if self.list_state.selected() == Some(entry_idx) {
                    self.on_left_selection_changed();
                }
            }
        }
    }
}

/// Формат длительности как строки с дискретностью 5 секунд:
///   5s, 10s, ..., 55s, 1m, 2m, ..., 4m, после 5m — пусто.
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 300 {
        return String::new();
    }
    let rounded = (secs / 5) * 5;
    if rounded < 60 {
        format!("{rounded}s")
    } else {
        format!("{}m", rounded / 60)
    }
}
