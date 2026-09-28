//! Load / save document data as YAML or shell `export` (shebang-detected).

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_yaml::{Mapping, Value};

/// Как читать и писать файл данных.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataFormat {
    Yaml,
    /// Shell-скрипт: исходный текст сохраняется, правятся только простые `export KEY=…`.
    Shell {
        source: String,
    },
}

#[derive(Debug, Clone)]
pub struct LoadedData {
    pub map: Mapping,
    pub format: DataFormat,
}

pub fn load(path: &Path) -> Result<LoadedData> {
    if !path.exists() {
        return Ok(LoadedData {
            map: Mapping::new(),
            format: DataFormat::Yaml,
        });
    }
    if !path.is_file() {
        bail!("input не файл: {}", path.display());
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("не удалось прочитать {}", path.display()))?;
    parse_text(&text).with_context(|| format!("ошибка разбора {}", path.display()))
}

/// `managed_keys` — ключи из схемы: их `export` обновляются/удаляются;
/// остальные строки (и «чужие» export) остаются на месте.
pub fn save(
    path: &Path,
    map: &Mapping,
    format: &DataFormat,
    managed_keys: &[String],
) -> Result<()> {
    let text = dump_text(map, format, managed_keys)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("не удалось создать {}", parent.display()))?;
        }
    }
    fs::write(path, text).with_context(|| format!("не удалось записать {}", path.display()))
}

pub fn parse_text(text: &str) -> Result<LoadedData> {
    if text.trim().is_empty() {
        return Ok(LoadedData {
            map: Mapping::new(),
            format: DataFormat::Yaml,
        });
    }

    let first = text.lines().next().unwrap_or("");
    if is_shell_shebang(first) {
        let map = parse_shell_map(text)?;
        return Ok(LoadedData {
            map,
            format: DataFormat::Shell {
                source: text.to_string(),
            },
        });
    }

    let root: Value = serde_yaml::from_str(text).context("невалидный YAML")?;
    let map = match root {
        Value::Mapping(m) => m,
        Value::Null => Mapping::new(),
        other => bail!(
            "корень input должен быть YAML-объектом, получено {}",
            type_name(&other)
        ),
    };
    Ok(LoadedData {
        map,
        format: DataFormat::Yaml,
    })
}

pub fn dump_text(map: &Mapping, format: &DataFormat, managed_keys: &[String]) -> Result<String> {
    match format {
        DataFormat::Yaml => {
            serde_yaml::to_string(&Value::Mapping(map.clone())).context("сериализация YAML")
        }
        DataFormat::Shell { source } => Ok(dump_shell_preserving(source, map, managed_keys)),
    }
}

/// Shebang shell-скрипта: строка начинается с `#!` и оканчивается на `/bash` или `/env bash`.
pub fn is_shell_shebang(line: &str) -> bool {
    let line = line.trim_end_matches(['\r', '\n']);
    if !line.starts_with("#!") {
        return false;
    }
    line.ends_with("/bash") || line.ends_with("/env bash")
}

/// Простой `export KEY=value` (после trim). Сложные строки (`cmd && export …`) — `None`.
fn parse_simple_export(line: &str) -> Option<(&str, &str, &str)> {
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    let indent = &line[..indent_len];
    let rest = trimmed.strip_prefix("export ")?.trim_start();
    let (key, value) = rest.split_once('=')?;
    let key = key.trim();
    if key.is_empty() || key.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    Some((indent, key, value.trim()))
}

fn parse_shell_map(text: &str) -> Result<Mapping> {
    let mut map = Mapping::new();
    for (lineno, line) in text.lines().enumerate() {
        if lineno == 0 && is_shell_shebang(line) {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((_, key, raw_value)) = parse_simple_export(line) else {
            continue;
        };
        let value =
            unquote_shell_value(raw_value).with_context(|| format!("строка {}", lineno + 1))?;
        map.insert(Value::String(key.to_string()), infer_scalar(&value));
    }
    Ok(map)
}

fn dump_shell_preserving(source: &str, map: &Mapping, managed_keys: &[String]) -> String {
    let managed: HashSet<&str> = managed_keys.iter().map(String::as_str).collect();
    let mut pending: Mapping = map.clone();
    let ends_with_newline = source.ends_with('\n');

    let mut out = String::new();
    for line in source.lines() {
        if let Some((indent, key, _)) = parse_simple_export(line) {
            if managed.contains(key) {
                let key_v = Value::String(key.to_string());
                if let Some(v) = pending.remove(&key_v) {
                    if let Some(s) = value_as_export_string(&v) {
                        out.push_str(indent);
                        out.push_str("export ");
                        out.push_str(key);
                        out.push('=');
                        out.push_str(&shell_quote(&s));
                        out.push('\n');
                    }
                }
                // managed, но нет в map → строку удаляем (undefined).
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }

    // Новые ключи схемы, которых не было в файле.
    for key in managed_keys {
        let key_v = Value::String(key.clone());
        if let Some(v) = pending.remove(&key_v) {
            if let Some(s) = value_as_export_string(&v) {
                out.push_str("export ");
                out.push_str(key);
                out.push('=');
                out.push_str(&shell_quote(&s));
                out.push('\n');
            }
        }
    }

    if !ends_with_newline && out.ends_with('\n') {
        out.pop();
    }
    out
}

fn unquote_shell_value(raw: &str) -> Result<String> {
    if raw.is_empty() {
        return Ok(String::new());
    }
    let bytes = raw.as_bytes();
    match bytes[0] {
        b'"' => {
            if bytes.len() < 2 || bytes[bytes.len() - 1] != b'"' {
                bail!("незакрытая двойная кавычка в значении: `{raw}`");
            }
            Ok(unescape_double(&raw[1..raw.len() - 1]))
        }
        b'\'' => {
            if bytes.len() < 2 || bytes[bytes.len() - 1] != b'\'' {
                bail!("незакрытая одинарная кавычка в значении: `{raw}`");
            }
            Ok(raw[1..raw.len() - 1].to_string())
        }
        _ => Ok(raw.to_string()),
    }
}

fn unescape_double(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(n @ ('\\' | '"' | '$' | '`' | '\n')) => out.push(n),
                Some(n) => {
                    out.push('\\');
                    out.push(n);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".to_string();
    }
    if s.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '_' | '-' | '.' | '/' | ':' | '@' | '%' | '+' | '=')
    }) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' | '"' | '$' | '`' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn infer_scalar(s: &str) -> Value {
    match s {
        "true" | "True" => Value::Bool(true),
        "false" | "False" => Value::Bool(false),
        _ => {
            if let Ok(i) = s.parse::<i64>() {
                Value::Number(i.into())
            } else if let Ok(f) = s.parse::<f64>() {
                if s.contains('.') || s.contains('e') || s.contains('E') {
                    Value::Number(serde_yaml::Number::from(f))
                } else {
                    Value::String(s.to_string())
                }
            } else {
                Value::String(s.to_string())
            }
        }
    }
}

fn value_as_export_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Null => Some(String::new()),
        _ => None,
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Sequence(_) => "sequence",
        Value::Mapping(_) => "mapping",
        Value::Tagged(_) => "tagged",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_shebangs() {
        assert!(is_shell_shebang("#!/bin/bash"));
        assert!(is_shell_shebang("#!/usr/bin/bash"));
        assert!(is_shell_shebang("#!/usr/bin/env bash"));
        assert!(!is_shell_shebang("#!/bin/sh"));
        assert!(!is_shell_shebang("#!/usr/bin/env zsh"));
        assert!(!is_shell_shebang("name: Alice"));
    }

    #[test]
    fn parse_shell_values() {
        let text = "#!/usr/bin/env bash\nexport NAME=Alice\nexport ACTIVE=true\nexport NOTE=\"hello world\"\n";
        let loaded = parse_text(text).unwrap();
        assert!(matches!(loaded.format, DataFormat::Shell { .. }));
        assert_eq!(
            loaded.map.get(Value::String("NAME".into())),
            Some(&Value::String("Alice".into()))
        );
        assert_eq!(
            loaded.map.get(Value::String("ACTIVE".into())),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            loaded.map.get(Value::String("NOTE".into())),
            Some(&Value::String("hello world".into()))
        );
    }

    #[test]
    fn preserve_non_managed_lines() {
        let text = "\
#!/usr/bin/env bash

[ -d /home/kaineer/devel/kaineer/cli-rs/bin ] && export PATH=/home/kaineer/devel/kaineer/cli-rs/bin:$PATH
export PROJECT_PATH=/home/kaineer/devel/kaineer/cli-rs
export PROJECT_BIN=/home/kaineer/devel/kaineer/cli-rs/bin
export PROJECT_NAME=cli-rs
export PROJECT_CREATED=\"18.08.2026\"
";
        let loaded = parse_text(text).unwrap();
        let mut map = Mapping::new();
        map.insert(
            Value::String("PROJECT_PATH".into()),
            Value::String("/home/kaineer/devel/kaineer/cli-rs".into()),
        );
        map.insert(
            Value::String("PROJECT_BIN".into()),
            Value::String("/home/kaineer/devel/kaineer/cli-rs/bin".into()),
        );
        map.insert(
            Value::String("PROJECT_NAME".into()),
            Value::String("cli-rs2".into()),
        );
        let managed = vec![
            "PROJECT_PATH".into(),
            "PROJECT_BIN".into(),
            "PROJECT_NAME".into(),
        ];
        let out = dump_text(&map, &loaded.format, &managed).unwrap();
        let expected = "\
#!/usr/bin/env bash

[ -d /home/kaineer/devel/kaineer/cli-rs/bin ] && export PATH=/home/kaineer/devel/kaineer/cli-rs/bin:$PATH
export PROJECT_PATH=/home/kaineer/devel/kaineer/cli-rs
export PROJECT_BIN=/home/kaineer/devel/kaineer/cli-rs/bin
export PROJECT_NAME=cli-rs2
export PROJECT_CREATED=\"18.08.2026\"
";
        assert_eq!(out, expected);
    }

    #[test]
    fn remove_undefined_managed_export() {
        let text = "#!/usr/bin/env bash\nexport KEEP=1\nexport DROP=2\n";
        let loaded = parse_text(text).unwrap();
        let mut map = Mapping::new();
        map.insert(Value::String("KEEP".into()), Value::String("1".into()));
        let managed = vec!["KEEP".into(), "DROP".into()];
        let out = dump_text(&map, &loaded.format, &managed).unwrap();
        assert_eq!(out, "#!/usr/bin/env bash\nexport KEEP=1\n");
    }

    #[test]
    fn append_new_managed_export() {
        let text = "#!/usr/bin/env bash\nexport OLD=1\n";
        let loaded = parse_text(text).unwrap();
        let mut map = Mapping::new();
        map.insert(Value::String("OLD".into()), Value::String("1".into()));
        map.insert(Value::String("NEW".into()), Value::String("2".into()));
        let managed = vec!["OLD".into(), "NEW".into()];
        let out = dump_text(&map, &loaded.format, &managed).unwrap();
        assert_eq!(out, "#!/usr/bin/env bash\nexport OLD=1\nexport NEW=2\n");
    }

    #[test]
    fn yaml_unchanged() {
        let text = "name: Alice\nactive: true\n";
        let loaded = parse_text(text).unwrap();
        assert_eq!(loaded.format, DataFormat::Yaml);
        assert_eq!(loaded.map.len(), 2);
    }
}
