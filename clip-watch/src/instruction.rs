use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Действие над файлом.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Write,
    Remove,
    Touch,
}

impl Verb {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "write" => Some(Self::Write),
            "remove" => Some(Self::Remove),
            "touch" => Some(Self::Touch),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Remove => "remove",
            Self::Touch => "touch",
        }
    }
}

/// Диапазон строк (1-based, inclusive). `end == None` — до конца файла.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: usize,
    pub end: Option<usize>,
}

/// Одна инструкция из буфера обмена.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub verb: Verb,
    pub path: PathBuf,
    pub range: Option<LineRange>,
    pub content: Option<String>,
}

impl Instruction {
    pub fn summary(&self) -> String {
        let range = match self.range {
            Some(LineRange {
                start,
                end: Some(end),
            }) => format!(":{start}-{end}"),
            Some(LineRange { start, end: None }) => format!(":{start}"),
            None => String::new(),
        };
        format!("{} {}{}", self.verb.as_str(), self.path.display(), range)
    }
}

/// Набор инструкций, готовый к применению.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstructionSet {
    pub instructions: Vec<Instruction>,
}

impl InstructionSet {
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    /// Уникальные пути в порядке первого появления.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut seen = BTreeMap::new();
        let mut order = Vec::new();
        for insn in &self.instructions {
            if !seen.contains_key(&insn.path) {
                seen.insert(insn.path.clone(), ());
                order.push(insn.path.clone());
            }
        }
        order
    }

    /// Краткие описания операций по файлу.
    pub fn summaries_for(&self, path: &Path) -> Vec<String> {
        self.instructions
            .iter()
            .filter(|i| i.path == path)
            .map(|i| i.summary())
            .collect()
    }

    pub fn remove_file(&mut self, path: &Path) {
        self.instructions.retain(|i| i.path != path);
    }
}

/// Разбирает текст буфера обмена в набор инструкций.
///
/// Блоки вида:
/// ```text
/// ### verb path[:start[-end]]
///
/// необязательные абзацы пояснений (игнорируются)
///
/// ```lang
/// code
/// ```
/// ```
///
/// Для `write` code fence обязателен (после пояснений).
/// Для `remove`/`touch` и пояснения, и код игнорируются.
pub fn parse(text: &str) -> InstructionSet {
    let lines: Vec<&str> = text.lines().collect();
    let mut instructions = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let Some(header) = parse_header(lines[i]) else {
            i += 1;
            continue;
        };

        i += 1;

        match header.verb {
            Verb::Write => {
                let (content, next) = take_code_block_after_prose(&lines, i);
                i = next;
                let Some(content) = content else {
                    continue;
                };
                instructions.push(Instruction {
                    verb: Verb::Write,
                    path: header.path,
                    range: header.range,
                    content: Some(content),
                });
            }
            Verb::Remove => {
                i = skip_until_header(&lines, i);
                instructions.push(Instruction {
                    verb: Verb::Remove,
                    path: header.path,
                    range: header.range,
                    content: None,
                });
            }
            Verb::Touch => {
                i = skip_until_header(&lines, i);
                instructions.push(Instruction {
                    verb: Verb::Touch,
                    path: header.path,
                    range: None,
                    content: None,
                });
            }
        }
    }

    InstructionSet { instructions }
}

struct Header {
    verb: Verb,
    path: PathBuf,
    range: Option<LineRange>,
}

fn parse_header(line: &str) -> Option<Header> {
    let rest = line.strip_prefix("###")?.trim_start();
    let mut parts = rest.splitn(2, char::is_whitespace);
    let verb = Verb::parse(parts.next()?)?;
    let target = parts.next()?.trim();
    if target.is_empty() {
        return None;
    }
    let (path, range) = split_path_range(target);
    Some(Header {
        verb,
        path: PathBuf::from(path),
        range,
    })
}

fn split_path_range(target: &str) -> (&str, Option<LineRange>) {
    let Some(colon) = target.rfind(':') else {
        return (target, None);
    };
    let (path, range_str) = target.split_at(colon);
    let range_str = &range_str[1..];
    if path.is_empty() {
        return (target, None);
    }
    match parse_line_range(range_str) {
        Some(range) => (path, Some(range)),
        None => (target, None),
    }
}

fn parse_line_range(s: &str) -> Option<LineRange> {
    if let Some((a, b)) = s.split_once('-') {
        let start: usize = a.parse().ok()?;
        let end: usize = b.parse().ok()?;
        if start == 0 || end == 0 || end < start {
            return None;
        }
        Some(LineRange {
            start,
            end: Some(end),
        })
    } else {
        let start: usize = s.parse().ok()?;
        if start == 0 {
            return None;
        }
        Some(LineRange { start, end: None })
    }
}

/// Пропускает пояснения после заголовка и читает ближайший fenced code block.
/// Останавливается перед следующим заголовком инструкции (не поглощая его).
fn take_code_block_after_prose(lines: &[&str], mut start: usize) -> (Option<String>, usize) {
    while start < lines.len() {
        if parse_header(lines[start]).is_some() {
            return (None, start);
        }
        if lines[start].trim_end().starts_with("```") {
            return read_fenced_block(lines, start);
        }
        start += 1;
    }
    (None, start)
}

/// Пропускает пояснения и любые code fence до следующего заголовка инструкции.
fn skip_until_header(lines: &[&str], mut start: usize) -> usize {
    while start < lines.len() {
        if parse_header(lines[start]).is_some() {
            return start;
        }
        if lines[start].trim_end().starts_with("```") {
            let (_, next) = read_fenced_block(lines, start);
            start = next;
            continue;
        }
        start += 1;
    }
    start
}

/// Читает fenced code block, начиная со строки открытия ```.
fn read_fenced_block(lines: &[&str], mut start: usize) -> (Option<String>, usize) {
    debug_assert!(lines.get(start).is_some_and(|l| l.trim_end().starts_with("```")));
    start += 1;
    let mut body = Vec::new();
    while start < lines.len() {
        if lines[start].trim_end().starts_with("```") {
            start += 1;
            return (Some(body.join("\n")), start);
        }
        body.push(lines[start]);
        start += 1;
    }
    // Незакрытый блок — невалиден.
    (None, start)
}

/// Применяет все инструкции набора относительно `base`.
pub fn apply_all(set: &InstructionSet, base: &Path) -> Result<(), ApplyError> {
    for insn in &set.instructions {
        apply_one(insn, base)?;
    }
    Ok(())
}

/// Применяет только инструкции для указанного файла.
pub fn apply_file(set: &InstructionSet, base: &Path, path: &Path) -> Result<(), ApplyError> {
    for insn in set.instructions.iter().filter(|i| i.path == path) {
        apply_one(insn, base)?;
    }
    Ok(())
}

#[derive(Debug)]
pub enum ApplyError {
    Io { path: PathBuf, source: io::Error },
    Message(String),
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {}", path.display(), source),
            Self::Message(msg) => write!(f, "{msg}"),
        }
    }
}

fn apply_one(insn: &Instruction, base: &Path) -> Result<(), ApplyError> {
    let full = base.join(&insn.path);
    match insn.verb {
        Verb::Touch => {
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).map_err(|source| ApplyError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            if !full.exists() {
                fs::write(&full, "").map_err(|source| ApplyError::Io {
                    path: full.clone(),
                    source,
                })?;
            } else {
                // Обновляем mtime, как touch(1).
                let file = fs::OpenOptions::new()
                    .write(true)
                    .open(&full)
                    .map_err(|source| ApplyError::Io {
                        path: full.clone(),
                        source,
                    })?;
                file.set_modified(std::time::SystemTime::now())
                    .map_err(|source| ApplyError::Io {
                        path: full.clone(),
                        source,
                    })?;
            }
            Ok(())
        }
        Verb::Remove => match insn.range {
            None => {
                if full.exists() {
                    fs::remove_file(&full).map_err(|source| ApplyError::Io {
                        path: full.clone(),
                        source,
                    })?;
                }
                Ok(())
            }
            Some(range) => replace_range(&full, range, ""),
        },
        Verb::Write => {
            let content = insn.content.as_deref().unwrap_or("");
            match insn.range {
                None => {
                    if let Some(parent) = full.parent() {
                        fs::create_dir_all(parent).map_err(|source| ApplyError::Io {
                            path: parent.to_path_buf(),
                            source,
                        })?;
                    }
                    let mut text = content.to_string();
                    if !text.is_empty() && !text.ends_with('\n') {
                        text.push('\n');
                    }
                    fs::write(&full, text).map_err(|source| ApplyError::Io {
                        path: full.clone(),
                        source,
                    })
                }
                Some(range) => replace_range(&full, range, content),
            }
        }
    }
}

fn replace_range(path: &Path, range: LineRange, replacement: &str) -> Result<(), ApplyError> {
    let existing = if path.exists() {
        fs::read_to_string(path).map_err(|source| ApplyError::Io {
            path: path.to_path_buf(),
            source,
        })?
    } else {
        String::new()
    };

    let lines: Vec<&str> = existing.lines().collect();
    let start_idx = range.start.saturating_sub(1);
    let end_idx = match range.end {
        Some(end) => end.min(lines.len()),
        None => lines.len(),
    };

    if start_idx > lines.len() {
        return Err(ApplyError::Message(format!(
            "{}: строка {} за пределами файла ({} строк)",
            path.display(),
            range.start,
            lines.len()
        )));
    }

    let after_idx = if end_idx > start_idx {
        end_idx
    } else {
        start_idx
    };

    let trimmed = replacement.strip_suffix('\n').unwrap_or(replacement);
    let repl_lines: Vec<&str> = if trimmed.is_empty() {
        Vec::new()
    } else {
        trimmed.lines().collect()
    };

    let mut out = Vec::with_capacity(start_idx + repl_lines.len() + (lines.len() - after_idx));
    out.extend(lines[..start_idx].iter().copied());
    out.extend(repl_lines);
    out.extend(lines[after_idx..].iter().copied());

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ApplyError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let mut text = out.join("\n");
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    } else if text.is_empty() && (existing.ends_with('\n') || existing.is_empty()) {
        // пустой файл после очистки — оставляем пустым
    }

    fs::write(path, text).map_err(|source| ApplyError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_write_full_file() {
        let text = "\
### write src/main.rs
```rust
fn main() {}
```
";
        let set = parse(text);
        assert_eq!(set.len(), 1);
        let i = &set.instructions[0];
        assert_eq!(i.verb, Verb::Write);
        assert_eq!(i.path, PathBuf::from("src/main.rs"));
        assert_eq!(i.range, None);
        assert_eq!(i.content.as_deref(), Some("fn main() {}"));
    }

    #[test]
    fn parse_write_range() {
        let text = "\
### write foo.rs:10-20
```
line
```
";
        let set = parse(text);
        let i = &set.instructions[0];
        assert_eq!(
            i.range,
            Some(LineRange {
                start: 10,
                end: Some(20)
            })
        );
    }

    #[test]
    fn parse_write_from_start() {
        let text = "\
### write foo.rs:5
```
x
```
";
        let set = parse(text);
        assert_eq!(
            set.instructions[0].range,
            Some(LineRange {
                start: 5,
                end: None
            })
        );
    }

    #[test]
    fn write_requires_code_block() {
        let text = "### write foo.rs\n";
        assert!(parse(text).is_empty());
    }

    #[test]
    fn write_allows_prose_before_code() {
        let text = "\
### write foo.rs:1-2

Здесь пояснение, почему меняем файл.

Ещё один абзац.

```
alpha
beta
```
";
        let set = parse(text);
        assert_eq!(set.len(), 1);
        let i = &set.instructions[0];
        assert_eq!(i.verb, Verb::Write);
        assert_eq!(i.content.as_deref(), Some("alpha\nbeta"));
        assert_eq!(
            i.range,
            Some(LineRange {
                start: 1,
                end: Some(2)
            })
        );
    }

    #[test]
    fn write_prose_without_code_is_skipped() {
        let text = "\
### write foo.rs

Только текст, без кода.

### remove bar.rs
";
        let set = parse(text);
        assert_eq!(set.len(), 1);
        assert_eq!(set.instructions[0].verb, Verb::Remove);
        assert_eq!(set.instructions[0].path, PathBuf::from("bar.rs"));
    }

    #[test]
    fn remove_ignores_prose_and_code() {
        let text = "\
### remove old.rs:1-3

Удаляем устаревший фрагмент.

```
ignored
```

### touch keep.txt
";
        let set = parse(text);
        assert_eq!(set.len(), 2);
        assert_eq!(set.instructions[0].verb, Verb::Remove);
        assert!(set.instructions[0].content.is_none());
        assert_eq!(set.instructions[1].verb, Verb::Touch);
    }

    #[test]
    fn touch_ignores_range_and_code() {
        let text = "\
### touch new.txt:1-2
```
nope
```
";
        let set = parse(text);
        let i = &set.instructions[0];
        assert_eq!(i.verb, Verb::Touch);
        assert_eq!(i.range, None);
        assert!(i.content.is_none());
    }

    #[test]
    fn multiple_instructions() {
        let text = "\
### write a.rs
```
a
```
### remove b.rs
### touch c.rs
";
        let set = parse(text);
        assert_eq!(set.len(), 3);
        assert_eq!(
            set.files(),
            vec![
                PathBuf::from("a.rs"),
                PathBuf::from("b.rs"),
                PathBuf::from("c.rs")
            ]
        );
    }

    #[test]
    fn apply_write_and_remove_range() {
        let dir = std::env::temp_dir().join(format!("clip-watch-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let set = parse(
            "\
### write demo.txt
```
one
two
three
four
```
",
        );
        apply_all(&set, &dir).unwrap();
        let path = dir.join("demo.txt");
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\ntwo\nthree\nfour\n");

        let set = parse(
            "\
### write demo.txt:2-3
```
TWO
THREE
```
",
        );
        apply_all(&set, &dir).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\nTWO\nTHREE\nfour\n");

        let set = parse("### remove demo.txt:2-3\n");
        apply_all(&set, &dir).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\nfour\n");

        let set = parse("### remove demo.txt\n");
        apply_all(&set, &dir).unwrap();
        assert!(!path.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
