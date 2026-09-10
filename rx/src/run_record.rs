use std::time::{Duration, SystemTime};

/// Один запуск команды.
#[derive(Debug, Clone)]
pub struct RunRecord {
    /// Индекс элемента в `App::entries`, к которому относится запуск.
    pub entry_idx: usize,
    /// Когда стартовали.
    pub started_at: SystemTime,
    /// Когда процесс завершился.
    pub finished_at: SystemTime,
    /// Сколько длилось выполнение.
    pub duration: Duration,
    /// Код выхода (None, если процесс убит сигналом).
    pub exit_code: Option<i32>,
    /// Успех = exit_code == Some(0).
    pub ok: bool,
    /// Собранный stdout (UTF-8, lossy).
    pub stdout: String,
    /// Собранный stderr (UTF-8, lossy).
    pub stderr: String,
}
