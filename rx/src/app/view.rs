use std::time::Duration;

use crate::run_record::RunRecord;

/// Сколько строк показывать в Preview.
pub const PREVIEW_LINES: usize = 12;

/// Степень раскрытия одного запуска.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunView {
    #[default]
    Collapsed,
    Preview,
    Full,
}

/// Per-run UI-состояние.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunViewState {
    pub view: RunView,
    pub scroll: usize,
}

/// Высота блока одного запуска в строках при данном виде.
pub fn block_height(run: &RunRecord, view: RunView) -> usize {
    let total = run.stdout.lines().count();
    match view {
        RunView::Collapsed => 1,
        RunView::Preview => {
            let shown = total.min(PREVIEW_LINES);
            let tail = if total > PREVIEW_LINES { 1 } else { 0 };
            1 + shown + tail
        }
        RunView::Full => 1 + total,
    }
}

/// Формат длительности как строки с дискретностью 5 секунд:
///   5s, 10s, 15s, ..., 55s, 1m, 2m, ..., 4m, после 5m — пусто.
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
