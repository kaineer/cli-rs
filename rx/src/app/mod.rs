pub mod process;
pub mod view;

use ratatui::layout::Rect;
use ratatui::widgets::ListState;

use crate::config::MenuEntry;
use crate::run_record::RunRecord;

pub use process::FinishedRun;
pub use view::{block_height, format_duration, RunView, RunViewState, PREVIEW_LINES};

use process::ProcessPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Left,
    Right,
}

/// Подрежим правой панели, вычисляется от состояния выделенного запуска.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightMode {
    /// Ни один запуск не раскрыт: j/k листают список.
    List,
    /// Выделенный раскрыт: j/k скроллят содержимое (в Full).
    Body,
}

/// Рантайм-состояние одной записи левого списка.
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
    /// Индекс первого видимого блока в правой панели (в терминах run_idxs).
    pub right_first_visible: usize,
    pub focus: Panel,
    pub left_width: u16,
    pub dragging: bool,
    pub area_width: u16,
    /// Высота правой панели, обновляется в ui::draw_right.
    pub right_area_height: u16,
    /// Прямоугольник левой панели, обновляется в ui::draw.
    pub left_panel_rect: Rect,
    /// Прямоугольник правой панели, обновляется в ui::draw.
    pub right_panel_rect: Rect,
    pub status: String,
    pub should_quit: bool,
    pub tick: u64,

    /// История всех запусков в порядке появления.
    pub runs: Vec<RunRecord>,
    /// Параллельный `runs` вектор per-run UI-состояния.
    pub run_views: Vec<RunViewState>,

    /// Активные процессы.
    processes: ProcessPool,
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
            right_first_visible: 0,
            focus: Panel::Left,
            left_width: 12,
            dragging: false,
            area_width: 80,
            right_area_height: 20,
            left_panel_rect: Rect::default(),
            right_panel_rect: Rect::default(),
            status: String::new(),
            should_quit: false,
            tick: 0,
            runs: Vec::new(),
            run_views: Vec::new(),
            processes: ProcessPool::new(),
        }
    }

    // ---- левая панель ----

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

    pub fn activate_selected(&mut self) {
        if let Some(idx) = self.list_state.selected() {
            self.activate(idx);
        }
    }

    // ---- мышь ----

    /// Индекс элемента левого списка по координате `row` (строка в терминале).
    /// Возвращает None, если строка не попадает в видимую область списка
    /// или за пределами количества элементов.
    fn left_index_at_row(&self, row: u16) -> Option<usize> {
        if row < self.left_panel_rect.y {
            return None;
        }
        let rel = (row - self.left_panel_rect.y) as usize;
        if rel >= self.left_panel_rect.height as usize {
            return None;
        }
        let idx = self.list_state.offset() + rel;
        if idx < self.entries.len() {
            Some(idx)
        } else {
            None
        }
    }

    /// Левый клик по левой панели: переключить фокус влево и выделить элемент.
    pub fn on_left_panel_click(&mut self, row: u16) {
        if let Some(idx) = self.left_index_at_row(row) {
            self.list_state.select(Some(idx));
            self.on_left_selection_changed();
        }
        self.focus = Panel::Left;
    }

    /// Правый клик по левой панели: выделить элемент и запустить его.
    /// Если клик пришёлся мимо элемента — только переключаем фокус влево.
    pub fn on_left_panel_right_click(&mut self, row: u16) {
        if let Some(idx) = self.left_index_at_row(row) {
            self.list_state.select(Some(idx));
            self.on_left_selection_changed();
            self.focus = Panel::Left;
            self.activate_selected();
        } else {
            self.focus = Panel::Left;
        }
    }

    /// Левый клик по правой панели: переключить фокус вправо и (в режиме List)
    /// выделить запуск по строке.
    pub fn on_right_panel_click(&mut self, row: u16) {
        self.focus = Panel::Right;

        // В Body строки переменной высоты — в конкретный запуск не попадаем.
        if self.right_mode() != RightMode::List {
            return;
        }
        let run_idxs = self.runs_for_selected();
        if run_idxs.is_empty() {
            return;
        }
        if row < self.right_panel_rect.y {
            return;
        }
        let rel = (row - self.right_panel_rect.y) as usize;
        if rel >= self.right_panel_rect.height as usize {
            return;
        }
        let idx = self.right_first_visible + rel;
        if idx < run_idxs.len() {
            self.right_state.select(Some(idx));
        }
    }

    // ---- переключение фокуса ----

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Panel::Left => {
                let n = self.runs_for_selected().len();
                if n == 0 {
                    return;
                }
                if self.right_state.selected().is_none() {
                    self.right_state.select(Some(0));
                }
                Panel::Right
            }
            Panel::Right => {
                // Уходим влево: схлопываем всё раскрытое.
                self.collapse_all_views_except(None);
                Panel::Left
            }
        };
    }

    // ---- правая панель ----

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

    /// run_id и view текущего выделенного в правой панели запуска.
    fn right_selected_run(&self) -> Option<(usize, RunView)> {
        let sel = self.right_state.selected()?;
        let &run_id = self.runs_for_selected().get(sel)?;
        Some((run_id, self.run_views[run_id].view))
    }

    /// Текущий подрежим правой панели.
    pub fn right_mode(&self) -> RightMode {
        match self.right_selected_run() {
            Some((_, RunView::Collapsed)) => RightMode::List,
            Some(_) => RightMode::Body,
            None => RightMode::List,
        }
    }

    /// true, если сейчас в правой панели режим Body.
    pub fn in_body_mode(&self) -> bool {
        self.right_mode() == RightMode::Body
    }

    fn on_left_selection_changed(&mut self) {
        self.right_state = ListState::default();
        self.right_first_visible = 0;
        self.collapse_all_views_except(None);
        let n = self.runs_for_selected().len();
        if n > 0 {
            self.right_state.select(Some(0));
        }
    }

    /// Схлопнуть все раскрытые запуски в `run_views`, кроме `keep`.
    fn collapse_all_views_except(&mut self, keep: Option<usize>) {
        for (i, vs) in self.run_views.iter_mut().enumerate() {
            if Some(i) != keep && vs.view != RunView::Collapsed {
                vs.view = RunView::Collapsed;
                vs.scroll = 0;
            }
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

    /// Space в правой панели: Collapsed → Preview → Full → Collapsed.
    /// Если строк ≤ PREVIEW_LINES, Full недоступен.
    pub fn right_toggle_view(&mut self) {
        let Some(sel) = self.right_state.selected() else {
            return;
        };
        let Some(&run_id) = self.runs_for_selected().get(sel) else {
            return;
        };

        let total = self.runs[run_id].stdout.lines().count();
        let can_expand = total > PREVIEW_LINES;

        let current = self.run_views[run_id].view;
        let next = match current {
            RunView::Collapsed => RunView::Preview,
            RunView::Preview => {
                if can_expand {
                    RunView::Full
                } else {
                    RunView::Collapsed
                }
            }
            RunView::Full => RunView::Collapsed,
        };

        if next == RunView::Collapsed {
            self.collapse_all_views_except(None);
        } else {
            self.collapse_all_views_except(Some(run_id));
            self.run_views[run_id].scroll = 0;
        }
        self.run_views[run_id].view = next;
    }

    /// Прокрутка содержимого выделенного раскрытого запуска.
    /// Скролл есть только в Full: Preview фиксирован (первые PREVIEW_LINES),
    /// Collapsed вообще без содержимого.
    pub fn right_scroll(&mut self, delta: i64) {
        let Some(sel) = self.right_state.selected() else {
            return;
        };
        let Some(&run_id) = self.runs_for_selected().get(sel) else {
            return;
        };

        let state = &mut self.run_views[run_id];

        let content_rows = match state.view {
            RunView::Collapsed | RunView::Preview => return,
            RunView::Full => self.runs[run_id].stdout.lines().count(),
        };

        let viewport = (self.right_area_height as usize).saturating_sub(1);
        let max_scroll = content_rows.saturating_sub(viewport);
        let new = (state.scroll as i64 + delta).clamp(0, max_scroll as i64);
        state.scroll = new as usize;
    }

    /// Эффективная высота блока i_in_run_idxs (в терминах индексов списка run_idxs).
    /// В Body все элементы, кроме выделенного, схлопываются.
    fn effective_block_height(
        &self,
        run_idxs: &[usize],
        i: usize,
        sel: usize,
        in_body: bool,
    ) -> usize {
        let run_id = run_idxs[i];
        let view = if in_body && i != sel {
            RunView::Collapsed
        } else {
            self.run_views[run_id].view
        };
        block_height(&self.runs[run_id], view)
    }

    /// Пересчитывает `right_first_visible` так, чтобы:
    ///   - выделенный элемент был виден хотя бы шапкой;
    ///   - всё, что выше выделенного, показывалось, если место есть.
    pub fn recompute_right_first(&mut self) {
        let run_idxs = self.runs_for_selected();
        if run_idxs.is_empty() {
            self.right_first_visible = 0;
            return;
        }
        let sel = self
            .right_state
            .selected()
            .unwrap_or(0)
            .min(run_idxs.len() - 1);
        let h = self.right_area_height as usize;
        let in_body = self.in_body_mode();

        if sel < self.right_first_visible {
            self.right_first_visible = sel;
        }

        loop {
            let used: usize = (self.right_first_visible..=sel)
                .map(|i| self.effective_block_height(&run_idxs, i, sel, in_body))
                .sum();
            if used <= h || self.right_first_visible == sel {
                break;
            }
            self.right_first_visible += 1;
        }

        loop {
            if self.right_first_visible == 0 {
                break;
            }
            let cand = self.right_first_visible - 1;
            let used: usize = (cand..=sel)
                .map(|i| self.effective_block_height(&run_idxs, i, sel, in_body))
                .sum();
            if used <= h {
                self.right_first_visible = cand;
            } else {
                break;
            }
        }
    }

    // ---- тик и процессы ----

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);

        let finished = self.processes.poll();
        for f in finished {
            let record = RunRecord {
                entry_idx: f.entry_idx,
                started_at: f.started_at,
                finished_at: f.finished_at,
                duration: f.duration,
                exit_code: f.exit_code,
                signal: f.signal,
                ok: f.ok,
                stdout: f.stdout,
                stderr: f.stderr,
            };
            self.runs.push(record);
            self.run_views.push(RunViewState::default());
            let run_id = self.runs.len() - 1;
            self.states[f.entry_idx] = EntryState::Done { run_id };
            self.status = format!(
                "{} {}",
                self.entries[f.entry_idx].label,
                if f.ok { "ok" } else { "ошибка" },
            );
            if self.list_state.selected() == Some(f.entry_idx) {
                self.on_left_selection_changed();
            }
        }
    }

    fn activate(&mut self, idx: usize) {
        if self.processes.is_running(idx) {
            self.status = format!("Уже выполняется для {}", self.entries[idx].label);
            return;
        }

        let cmd = self.entries[idx].cmd.clone();
        match self.processes.spawn(idx, &cmd) {
            Ok(()) => {
                self.states[idx] = EntryState::Running;
                self.status = format!("Запущено: {}", self.entries[idx].label);
            }
            Err(e) => {
                self.status = format!("Ошибка запуска: {e}");
                let now = std::time::SystemTime::now();
                let record = RunRecord {
                    entry_idx: idx,
                    started_at: now,
                    finished_at: now,
                    duration: std::time::Duration::ZERO,
                    exit_code: None,
                    signal: None,
                    ok: false,
                    stdout: String::new(),
                    stderr: e.to_string(),
                };
                self.runs.push(record);
                self.run_views.push(RunViewState::default());
                let run_id = self.runs.len() - 1;
                self.states[idx] = EntryState::Done { run_id };
                if self.list_state.selected() == Some(idx) {
                    self.on_left_selection_changed();
                }
            }
        }
    }
}
