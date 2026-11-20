//! Запуск команды пункта меню в новой панели мультиплексора.
//!
//! Если `rx` живёт внутри tmux или herdr-app, команду можно отдать им:
//! вывод останется в отдельной панели, а меню не блокируется.

use std::env;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

/// Мультиплексор, в котором запущен `rx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Multiplexer {
    Tmux,
    Herdr,
}

/// Результат попытки открыть панель (для строки статуса).
pub enum SplitOutcome {
    Ok { label: String },
    Err { label: String, error: String },
}

/// Кто нас запустил. Проверяем окружение процесса.
pub fn detect() -> Option<Multiplexer> {
    detect_from(
        env::var_os("TMUX").is_some(),
        env::var_os("HC_TERMINAL_ID").is_some()
            || env::var_os("HC_TERMINAL_SOCKET").is_some(),
    )
}

/// Правило выбора: внутренний мультиплексор важнее внешнего — панель должна
/// появиться там, где пользователь видит rx.
fn detect_from(tmux: bool, herdr: bool) -> Option<Multiplexer> {
    if tmux {
        Some(Multiplexer::Tmux)
    } else if herdr {
        Some(Multiplexer::Herdr)
    } else {
        None
    }
}

/// argv для запуска команды в новой панели.
pub fn command_argv(mux: Multiplexer, cmd: &str) -> Vec<String> {
    match mux {
        Multiplexer::Herdr => vec![
            "herdr-app".into(),
            "do".into(),
            "pane.split".into(),
            "--direction".into(),
            "right".into(),
            "--argv".into(),
            json_array(&herdr_argv(cmd)),
        ],
        // tmux сам передаёт shell-command шеллу, поэтому строка идёт как есть.
        Multiplexer::Tmux => vec![
            "tmux".into(),
            "splitw".into(),
            "-v".into(),
            "-c".into(),
            "#{pane_current_dir}".into(),
            cmd.to_string(),
        ],
    }
}

/// herdr-app запускает `--argv` напрямую (`execvp`), без шелла.
/// Команды с пайпами, `&&`, подстановками и т.п. оборачиваем в `sh -c`,
/// остальные отдаём разбитыми на слова.
pub fn herdr_argv(cmd: &str) -> Vec<String> {
    if needs_shell(cmd) {
        vec!["sh".into(), "-c".into(), cmd.to_string()]
    } else {
        split_words(cmd)
    }
}

/// Есть ли в команде то, что понимает только шелл.
fn needs_shell(cmd: &str) -> bool {
    cmd.chars().any(|c| {
        matches!(
            c,
            '|' | '&' | ';' | '<' | '>' | '$' | '`' | '(' | ')' | '*' | '?' | '[' | ']' | '~' | '#' | '\n'
        )
    })
}

/// Разбивает команду на слова, понимая кавычки и `\`-экранирование.
/// Для простых команд совпадает с `split_whitespace`.
pub fn split_words(cmd: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = cmd.chars();

    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {
                cur.push(c);
                started = true;
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    started = true;
                }
                '\\' => {
                    if let Some(next) = chars.next() {
                        cur.push(next);
                        started = true;
                    }
                }
                c if c.is_whitespace() => {
                    if started {
                        words.push(std::mem::take(&mut cur));
                        started = false;
                    }
                }
                c => {
                    cur.push(c);
                    started = true;
                }
            },
        }
    }
    if started {
        words.push(cur);
    }
    words
}

/// JSON-массив строк — формат `--argv` у herdr-app.
pub fn json_array(words: &[String]) -> String {
    let mut out = String::from("[");
    for (i, w) in words.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        for c in w.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                }
                c => out.push(c),
            }
        }
        out.push('"');
    }
    out.push(']');
    out
}

/// Открывает панель в фоне; результат придёт в `tx`.
pub fn launch(mux: Multiplexer, cmd: &str, label: String, tx: Sender<SplitOutcome>) {
    let argv = command_argv(mux, cmd);
    thread::spawn(move || {
        let outcome = match run(&argv) {
            Ok(()) => SplitOutcome::Ok { label },
            Err(error) => SplitOutcome::Err { label, error },
        };
        let _ = tx.send(outcome);
    });
}

fn run(argv: &[String]) -> Result<(), String> {
    let Some((program, args)) = argv.split_first() else {
        return Err("пустая команда".into());
    };
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let mut msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if msg.is_empty() {
        msg = String::from_utf8_lossy(&out.stdout).trim().to_string();
    }
    if msg.is_empty() {
        msg = format!("код {}", out.status.code().unwrap_or(-1));
    }
    Err(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_prefers_inner_tmux() {
        assert_eq!(detect_from(true, true), Some(Multiplexer::Tmux));
        assert_eq!(detect_from(false, true), Some(Multiplexer::Herdr));
        assert_eq!(detect_from(false, false), None);
    }

    #[test]
    fn words_split_like_shell() {
        assert_eq!(split_words("echo hi"), vec!["echo", "hi"]);
        assert_eq!(
            split_words("sh -c 'printf \"a b\"'"),
            vec!["sh", "-c", "printf \"a b\""]
        );
        assert_eq!(split_words("a  b\t\nc"), vec!["a", "b", "c"]);
        assert_eq!(split_words("x\\ y"), vec!["x y"]);
        assert!(split_words("   ").is_empty());
    }

    #[test]
    fn json_escapes_specials() {
        let words = vec!["a\"b".to_string(), "c\\d".to_string()];
        assert_eq!(json_array(&words), r#"["a\"b","c\\d"]"#);
    }

    #[test]
    fn herdr_argv_is_shell_split() {
        let argv = command_argv(Multiplexer::Herdr, "/bin/ax sync --force");
        assert_eq!(
            argv,
            vec![
                "herdr-app",
                "do",
                "pane.split",
                "--direction",
                "right",
                "--argv",
                r#"["/bin/ax","sync","--force"]"#,
            ]
        );
    }

    #[test]
    fn herdr_wraps_shell_syntax() {
        let argv = command_argv(Multiplexer::Herdr, "echo hi | wc -l");
        assert_eq!(argv.last().unwrap(), r#"["sh","-c","echo hi | wc -l"]"#);

        let argv = command_argv(Multiplexer::Herdr, "cd /tmp && ls");
        assert_eq!(argv.last().unwrap(), r#"["sh","-c","cd /tmp && ls"]"#);
    }

    #[test]
    fn tmux_argv_keeps_command_whole() {
        let argv = command_argv(Multiplexer::Tmux, "echo hi | wc -l");
        assert_eq!(
            argv,
            vec![
                "tmux",
                "splitw",
                "-v",
                "-c",
                "#{pane_current_dir}",
                "echo hi | wc -l",
            ]
        );
    }
}
