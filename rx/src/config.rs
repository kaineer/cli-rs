use std::{fs::File, io::BufReader, path::PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Clone)]
pub struct MenuEntry {
    pub cmd: String,
    pub label: String,
    /// Показывать stdout в правой панели по мере выполнения.
    /// В YAML: `live: true` или алиас `stream: true`.
    #[serde(default, alias = "stream")]
    pub live: bool,
    /// Запускать в новой панели tmux/herdr-app, если rx запущен внутри них.
    /// По умолчанию включено.
    #[serde(default = "default_true")]
    pub split: bool,
}

pub fn load_entries(path: &PathBuf) -> Result<Vec<MenuEntry>> {
    let file = File::open(path)
        .with_context(|| format!("Не удалось открыть файл {:?}", path))?;
    let reader = BufReader::new(file);
    let entries: Vec<MenuEntry> =
        serde_yaml::from_reader(reader).with_context(|| "Не удалось распарсить YAML")?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_and_stream_alias() {
        let live: Vec<MenuEntry> =
            serde_yaml::from_str("- cmd: a\n  label: a\n  live: true\n").unwrap();
        let stream: Vec<MenuEntry> =
            serde_yaml::from_str("- cmd: b\n  label: b\n  stream: true\n").unwrap();
        let plain: Vec<MenuEntry> =
            serde_yaml::from_str("- cmd: c\n  label: c\n").unwrap();
        assert!(live[0].live);
        assert!(stream[0].live);
        assert!(!plain[0].live);
    }

    #[test]
    fn split_defaults_to_true() {
        let entries: Vec<MenuEntry> =
            serde_yaml::from_str("- cmd: a\n  label: a\n- cmd: b\n  label: b\n  split: false\n")
                .unwrap();
        assert!(entries[0].split);
        assert!(!entries[1].split);
    }
}
