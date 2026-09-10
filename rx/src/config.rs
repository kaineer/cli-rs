use std::{fs::File, io::BufReader, path::PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct MenuEntry {
    pub cmd: String,
    pub label: String,
}

pub fn load_entries(path: &PathBuf) -> Result<Vec<MenuEntry>> {
    let file = File::open(path)
        .with_context(|| format!("Не удалось открыть файл {:?}", path))?;
    let reader = BufReader::new(file);
    let entries: Vec<MenuEntry> =
        serde_yaml::from_reader(reader).with_context(|| "Не удалось распарсить YAML")?;
    Ok(entries)
}
