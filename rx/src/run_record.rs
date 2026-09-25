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
    /// Код возврата, если процесс завершился нормально (не сигналом).
    pub exit_code: Option<i32>,
    /// Номер сигнала, если процесс убит сигналом (Unix-only; на других ОС — None).
    pub signal: Option<i32>,
    /// Успех = exit_code == Some(0).
    pub ok: bool,
    /// Собранный stdout (UTF-8, lossy).
    pub stdout: String,
    /// Собранный stderr (UTF-8, lossy).
    pub stderr: String,
}

impl RunRecord {
    /// Строки тела для правой панели.
    ///
    /// Успех — только stdout.
    /// Ошибка — stderr (или `exit code: N`, если stderr пуст); если stdout
    /// непуст, после этого пустая строка и сам stdout.
    pub fn body_lines(&self) -> Vec<String> {
        if self.ok {
            return self.stdout.lines().map(str::to_string).collect();
        }

        let mut lines = Vec::new();
        if self.stderr.is_empty() {
            let code = self
                .exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".into());
            lines.push(format!("exit code: {code}"));
        } else {
            lines.extend(self.stderr.lines().map(str::to_string));
        }

        if !self.stdout.is_empty() {
            lines.push(String::new());
            lines.extend(self.stdout.lines().map(str::to_string));
        }

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn run(ok: bool, exit_code: Option<i32>, stdout: &str, stderr: &str) -> RunRecord {
        RunRecord {
            entry_idx: 0,
            started_at: SystemTime::UNIX_EPOCH,
            finished_at: SystemTime::UNIX_EPOCH,
            duration: Duration::ZERO,
            exit_code,
            signal: None,
            ok,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }

    #[test]
    fn ok_shows_only_stdout() {
        let r = run(true, Some(0), "a\nb\n", "err\n");
        assert_eq!(r.body_lines(), vec!["a", "b"]);
    }

    #[test]
    fn fail_empty_stderr_shows_exit_code() {
        let r = run(false, Some(2), "", "");
        assert_eq!(r.body_lines(), vec!["exit code: 2"]);
    }

    #[test]
    fn fail_stderr_then_blank_then_stdout() {
        let r = run(false, Some(1), "out\n", "err1\nerr2\n");
        assert_eq!(
            r.body_lines(),
            vec!["err1".into(), "err2".into(), String::new(), "out".into()]
        );
    }

    #[test]
    fn fail_stderr_only_no_blank() {
        let r = run(false, Some(1), "", "boom\n");
        assert_eq!(r.body_lines(), vec!["boom"]);
    }
}
