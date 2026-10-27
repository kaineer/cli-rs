pub mod process;
pub mod view;

use std::path::PathBuf;

use ratatui::layout::Rect;
use ratatui::widgets::ListState;

use crate::config::{load_entries, MenuEntry};
use crate::run_record::RunRecord;

pub use process::FinishedRun;
pub use view::{block_height, format_duration, RunView, RunViewState, PREVIEW_LINES};

use process::{LiveOutput, ProcessPool};

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
    menu_path: PathBuf,
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
    /// Ожидание второго `g` для chord `gg`.
    pub pending_g: bool,
    pub tick: u64,

    /// История всех запусков в порядке появления.
    pub runs: Vec<RunRecord>,
    /// Параллельный `runs` вектор per-run UI-состояния.
    pub run_views: Vec<RunViewState>,

    /// Активные процессы.
    processes: ProcessPool,
}

impl App {
    pub fn new(menu_path: PathBuf, entries: Vec<MenuEntry>) -> Self {
        let states = entries.iter().map(|_| EntryState::Idle).collect();
        let mut list_state = ListState::default();
        if !entries.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            menu_path,
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
            pending_g: false,
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

    pub fn go_first(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        if self.list_state.selected() == Some(0) {
            return;
        }
        self.list_state.select(Some(0));
        self.on_left_selection_changed();
    }

    pub fn go_last(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let last = self.entries.len() - 1;
        if self.list_state.selected() == Some(last) {
            return;
        }
        self.list_state.select(Some(last));
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

    /// Индекс запуска в списке `runs_for_selected()` по строке терминала `row`.
    /// Учитывает переменную высоту блоков так же, как `ui::draw_right`.
    fn right_list_index_at_row(&self, row: u16) -> Option<usize> {
        if row < self.right_panel_rect.y {
            return None;
        }
        let rel = (row - self.right_panel_rect.y) as usize;
        if rel >= self.right_panel_rect.height as usize {
            return None;
        }
        let run_idxs = self.runs_for_selected();
        if run_idxs.is_empty() {
            return None;
        }

        let sel = self
            .right_state
            .selected()
            .unwrap_or(0)
            .min(run_idxs.len() - 1);
        let in_body = self.in_body_mode();
        let max_rows = self.right_panel_rect.height as usize;
        let first = self.right_first_visible.min(run_idxs.len() - 1);

        let mut visual_row = 0usize;
        for (offset, &run_id) in run_idxs.iter().enumerate().skip(first) {
            let run = &self.runs[run_id];
            let is_selected = offset == sel;
            let effective_view = if in_body && !is_selected {
                RunView::Collapsed
            } else {
                self.run_views[run_id].view
            };

            let block_start = visual_row;
            visual_row += 1; // header
            if visual_row > max_rows {
                break;
            }
            if rel < visual_row {
                return Some(offset);
            }

            let total = run.body_lines().len();
            let body_rows = match effective_view {
                RunView::Collapsed => 0,
                RunView::Preview => total.min(PREVIEW_LINES),
                RunView::Full => total,
            };

            if body_rows > 0 {
                let vs = self.run_views[run_id];
                let all_len = total;
                let start = vs.scroll.min(all_len);
                let end = (start + body_rows).min(all_len);
                let shown_body = end.saturating_sub(start);
                visual_row += shown_body;
                if visual_row > max_rows {
                    if rel >= block_start && rel < max_rows.min(visual_row) {
                        return Some(offset);
                    }
                    break;
                }
                if rel < visual_row {
                    return Some(offset);
                }

                if effective_view == RunView::Preview && visual_row < max_rows {
                    let shown = body_rows;
                    let rest = total.saturating_sub(shown);
                    if rest > 0 {
                        visual_row += 1;
                        if rel < visual_row {
                            return Some(offset);
                        }
                    }
                }
            }

            if visual_row >= max_rows {
                break;
            }
        }
        None
    }

    /// Левый клик по правой панели: переключить фокус вправо и (в режиме List)
    /// выделить запуск по строке.
    pub fn on_right_panel_click(&mut self, row: u16) {
        self.focus = Panel::Right;

        // В Body строки переменной высоты — в конкретный запуск не попадаем.
        if self.right_mode() != RightMode::List {
            return;
        }
        if let Some(idx) = self.right_list_index_at_row(row) {
            self.right_state.select(Some(idx));
        }
    }

    /// Правый клик по правой панели: выделить запуск по строке;
    /// если клик по уже выделенному — цикл раскрытия, как Space.
    pub fn on_right_panel_right_click(&mut self, row: u16) {
        self.focus = Panel::Right;
        let Some(idx) = self.right_list_index_at_row(row) else {
            return;
        };
        let was_selected = self.right_state.selected() == Some(idx);
        self.right_state.select(Some(idx));
        if was_selected {
            self.right_toggle_view();
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

    pub fn right_go_first(&mut self) {
        let n = self.runs_for_selected().len();
        if n == 0 {
            return;
        }
        self.right_state.select(Some(0));
    }

    pub fn right_go_last(&mut self) {
        let n = self.runs_for_selected().len();
        if n == 0 {
            return;
        }
        self.right_state.select(Some(n - 1));
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

        let total = self.runs[run_id].body_lines().len();
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
            RunView::Full => self.runs[run_id].body_lines().len(),
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

    /// Перечитывает файл меню и обновляет левый список.
    /// Запуски и активные процессы остаются у пунктов с той же парой
    /// (label, cmd). Пункт, которого больше нет, отвязывается.
    pub fn reload_menu(&mut self) {
        match load_entries(&self.menu_path) {
            Ok(entries) => self.apply_reloaded(entries),
            Err(err) => {
                self.status = format!("Не удалось перечитать: {err}");
            }
        }
    }

    fn apply_reloaded(&mut self, new_entries: Vec<MenuEntry>) {
        let old_sel = self.list_state.selected();
        let mapping = match_entries(&self.entries, &new_entries);

        let old_states = std::mem::take(&mut self.states);
        let mut new_states: Vec<EntryState> =
            (0..new_entries.len()).map(|_| EntryState::Idle).collect();
        for (old_i, state) in old_states.into_iter().enumerate() {
            if let Some(new_i) = mapping.get(old_i).copied().flatten() {
                new_states[new_i] = state;
            }
        }
        self.states = new_states;

        for run in &mut self.runs {
            run.entry_idx = mapping
                .get(run.entry_idx)
                .copied()
                .flatten()
                .unwrap_or(usize::MAX);
        }
        self.processes.remap_entries(&mapping);

        let n = new_entries.len();
        self.entries = new_entries;

        match old_sel.and_then(|i| mapping.get(i).copied().flatten()) {
            Some(i) => {
                self.list_state.select(Some(i));
            }
            None if n > 0 => {
                self.list_state.select(Some(0));
                self.on_left_selection_changed();
            }
            None => {
                self.list_state.select(None);
                self.on_left_selection_changed();
            }
        }

        let name = self
            .menu_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("меню");
        self.status = format!("Перечитано {name}: {n} {}", plural_punkt(n));
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);

        let (lives, finished) = self.processes.poll();
        for live in lives {
            self.apply_live(live);
        }
        for f in finished {
            self.apply_finished(f);
        }
    }

    fn apply_live(&mut self, live: LiveOutput) {
        let run_id = live.run_id;
        let Some(run) = self.runs.get_mut(run_id) else {
            return;
        };
        if !run.live {
            return;
        }
        run.stdout = live.stdout;
        run.stderr = live.stderr;
        self.follow_live_scroll(run_id);
    }

    fn follow_live_scroll(&mut self, run_id: usize) {
        let vs = &mut self.run_views[run_id];
        if vs.view != RunView::Full {
            return;
        }
        let content_rows = self.runs[run_id].body_lines().len();
        let viewport = (self.right_area_height as usize).saturating_sub(1);
        vs.scroll = content_rows.saturating_sub(viewport);
    }

    fn apply_finished(&mut self, f: FinishedRun) {
        let run_id = if let Some(run_id) = f.run_id {
            if let Some(run) = self.runs.get_mut(run_id) {
                run.finished_at = f.finished_at;
                run.duration = f.duration;
                run.exit_code = f.exit_code;
                run.signal = f.signal;
                run.ok = f.ok;
                run.live = false;
                run.stdout = f.stdout;
                run.stderr = f.stderr;
            }
            run_id
        } else {
            let record = RunRecord {
                entry_idx: f.entry_idx,
                started_at: f.started_at,
                finished_at: f.finished_at,
                duration: f.duration,
                exit_code: f.exit_code,
                signal: f.signal,
                ok: f.ok,
                live: false,
                stdout: f.stdout,
                stderr: f.stderr,
            };
            self.runs.push(record);
            self.run_views.push(RunViewState::default());
            self.runs.len() - 1
        };

        if f.entry_idx < self.states.len() {
            self.states[f.entry_idx] = EntryState::Done { run_id };
            self.status = format!(
                "{} {}",
                self.entries[f.entry_idx].label,
                if f.ok { "ok" } else { "ошибка" },
            );
            if self.list_state.selected() == Some(f.entry_idx) {
                if f.run_id.is_some() {
                    // Live: запись уже в списке — только follow-scroll,
                    // не трогая выделение / раскрытие.
                    self.follow_live_scroll(run_id);
                } else {
                    self.on_left_selection_changed();
                }
            }
        }
    }

    fn activate(&mut self, idx: usize) {
        if self.processes.is_running(idx) {
            self.status = format!("Уже выполняется для {}", self.entries[idx].label);
            return;
        }

        let cmd = self.entries[idx].cmd.clone();
        let live = self.entries[idx].live;

        let run_id = if live {
            let now = std::time::SystemTime::now();
            let record = RunRecord {
                entry_idx: idx,
                started_at: now,
                finished_at: now,
                duration: std::time::Duration::ZERO,
                exit_code: None,
                signal: None,
                ok: true,
                live: true,
                stdout: String::new(),
                stderr: String::new(),
            };
            self.runs.push(record);
            self.run_views.push(RunViewState {
                view: RunView::Full,
                scroll: 0,
            });
            Some(self.runs.len() - 1)
        } else {
            None
        };

        match self.processes.spawn(idx, &cmd, run_id) {
            Ok(()) => {
                self.states[idx] = EntryState::Running;
                self.status = format!("Запущено: {}", self.entries[idx].label);
                if let Some(run_id) = run_id {
                    if self.list_state.selected() == Some(idx) {
                        self.on_left_selection_changed();
                        self.collapse_all_views_except(Some(run_id));
                        self.run_views[run_id].view = RunView::Full;
                        self.run_views[run_id].scroll = 0;
                    }
                }
            }
            Err(e) => {
                self.status = format!("Ошибка запуска: {e}");
                if let Some(run_id) = run_id {
                    if let Some(run) = self.runs.get_mut(run_id) {
                        run.live = false;
                        run.ok = false;
                        run.stderr = e.to_string();
                        run.finished_at = std::time::SystemTime::now();
                    }
                    self.states[idx] = EntryState::Done { run_id };
                } else {
                    let now = std::time::SystemTime::now();
                    let record = RunRecord {
                        entry_idx: idx,
                        started_at: now,
                        finished_at: now,
                        duration: std::time::Duration::ZERO,
                        exit_code: None,
                        signal: None,
                        ok: false,
                        live: false,
                        stdout: String::new(),
                        stderr: e.to_string(),
                    };
                    self.runs.push(record);
                    self.run_views.push(RunViewState::default());
                    let run_id = self.runs.len() - 1;
                    self.states[idx] = EntryState::Done { run_id };
                }
                if self.list_state.selected() == Some(idx) {
                    self.on_left_selection_changed();
                }
            }
        }
    }
}

/// Старый индекс → новый, по паре (label, cmd). Одинаковые пункты
/// сопоставляются по порядку, без повторного использования.
fn match_entries(old: &[MenuEntry], new: &[MenuEntry]) -> Vec<Option<usize>> {
    let mut used = vec![false; new.len()];
    old.iter()
        .map(|old_e| {
            new.iter().enumerate().find_map(|(j, e)| {
                if !used[j] && e.label == old_e.label && e.cmd == old_e.cmd {
                    used[j] = true;
                    Some(j)
                } else {
                    None
                }
            })
        })
        .collect()
}

fn plural_punkt(n: usize) -> &'static str {
    let n10 = n % 10;
    let n100 = n % 100;
    if n10 == 1 && n100 != 11 {
        "пункт"
    } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
        "пункта"
    } else {
        "пунктов"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str, cmd: &str) -> MenuEntry {
        MenuEntry {
            label: label.into(),
            cmd: cmd.into(),
            live: false,
        }
    }

    #[test]
    fn match_keeps_identity_across_reorder() {
        let old = vec![entry("a", "1"), entry("b", "2"), entry("c", "3")];
        let new = vec![entry("c", "3"), entry("a", "1"), entry("d", "4")];
        assert_eq!(match_entries(&old, &new), vec![Some(1), None, Some(0)]);
    }

    #[test]
    fn match_duplicate_pairs_in_order() {
        let old = vec![entry("a", "1"), entry("a", "2")];
        let new = vec![entry("a", "2"), entry("a", "1")];
        assert_eq!(match_entries(&old, &new), vec![Some(1), Some(0)]);
    }

    #[test]
    fn reload_updates_list_and_follows_selection() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("rx-menu-reload-{}.yaml", std::process::id()));
        std::fs::write(
            &path,
            "- cmd: echo a\n  label: a\n- cmd: echo b\n  label: b\n",
        )
        .unwrap();

        let entries = load_entries(&path).unwrap();
        let mut app = App::new(path.clone(), entries);
        app.list_state.select(Some(1));
        app.states[1] = EntryState::Done { run_id: 0 };
        app.runs.push(RunRecord {
            entry_idx: 1,
            started_at: std::time::SystemTime::UNIX_EPOCH,
            finished_at: std::time::SystemTime::UNIX_EPOCH,
            duration: std::time::Duration::ZERO,
            exit_code: Some(0),
            signal: None,
            ok: true,
            live: false,
            stdout: "ok".into(),
            stderr: String::new(),
        });
        app.run_views.push(RunViewState::default());

        std::fs::write(
            &path,
            "- cmd: echo c\n  label: c\n- cmd: echo b\n  label: b\n- cmd: echo a\n  label: a\n",
        )
        .unwrap();
        app.reload_menu();

        assert_eq!(
            app.entries
                .iter()
                .map(|e| e.label.as_str())
                .collect::<Vec<_>>(),
            vec!["c", "b", "a"]
        );
        assert_eq!(app.list_state.selected(), Some(1));
        assert!(matches!(app.states[1], EntryState::Done { run_id: 0 }));
        assert!(matches!(app.states[0], EntryState::Idle));
        assert_eq!(app.runs[0].entry_idx, 1);
        assert!(app.status.starts_with("Перечитано"));
        assert!(app.status.contains("3 пункта"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn live_shows_stdout_before_exit() {
        let path = std::env::temp_dir().join(format!(
            "rx-menu-live-{}.yaml",
            std::process::id()
        ));
        // Печатаем строку, ждём, потом выходим — успеем увидеть live-вывод.
        std::fs::write(
            &path,
            "- cmd: sh -c 'printf \"hello\\n\"; sleep 0.4'\n  label: s\n  live: true\n",
        )
        .unwrap();

        let entries = load_entries(&path).unwrap();
        assert!(entries[0].live);
        let mut app = App::new(path.clone(), entries);
        app.activate_selected();

        assert!(matches!(app.states[0], EntryState::Running));
        assert_eq!(app.runs.len(), 1);
        assert!(app.runs[0].live);
        assert_eq!(app.run_views[0].view, RunView::Full);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            app.tick();
            if app.runs[0].stdout.contains("hello") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        assert!(
            app.runs[0].live,
            "ещё должно выполняться, когда появился stdout"
        );
        assert!(
            app.runs[0].stdout.contains("hello"),
            "stdout={}",
            app.runs[0].stdout
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            app.tick();
            if !app.runs[0].live {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        assert!(!app.runs[0].live);
        assert!(app.runs[0].ok);
        assert!(matches!(app.states[0], EntryState::Done { run_id: 0 }));

        let _ = std::fs::remove_file(&path);
    }
}
