//! Каталог `$HOME/Music/ytdl`: meta-файлы и пути к mp3.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Meta {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub meta: Meta,
    pub mp3: PathBuf,
    pub meta_path: PathBuf,
}

pub fn music_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("нет переменной HOME")?;
    Ok(PathBuf::from(home).join("Music").join("ytdl"))
}

pub fn ensure_music_dir() -> Result<PathBuf> {
    let dir = music_dir()?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("не удалось создать {}", dir.display()))?;
    Ok(dir)
}

pub fn mp3_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.mp3"))
}

pub fn meta_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.mp3.meta"))
}

pub fn write_meta(dir: &Path, id: &str, meta: &Meta) -> Result<PathBuf> {
    let path = meta_path(dir, id);
    let text = serde_yaml::to_string(meta).context("сериализация meta")?;
    fs::write(&path, text).with_context(|| format!("запись {}", path.display()))?;
    Ok(path)
}

pub fn read_meta(path: &Path) -> Result<Meta> {
    let text = fs::read_to_string(path).with_context(|| format!("чтение {}", path.display()))?;
    serde_yaml::from_str(&text).with_context(|| format!("разбор {}", path.display()))
}

/// Все записи с валидным `.mp3.meta` (и существующим mp3, если он есть).
pub fn list_entries(dir: &Path) -> Result<Vec<Entry>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for item in fs::read_dir(dir).with_context(|| format!("чтение {}", dir.display()))? {
        let item = item?;
        let path = item.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        let Some(id) = name.strip_suffix(".mp3.meta") else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        let meta = match read_meta(&path) {
            Ok(m) => m,
            Err(err) => {
                eprintln!("пропуск {}: {err:#}", path.display());
                continue;
            }
        };
        entries.push(Entry {
            id: id.to_string(),
            meta,
            mp3: mp3_path(dir, id),
            meta_path: path,
        });
    }

    entries.sort_by(|a, b| {
        a.meta
            .title
            .to_lowercase()
            .cmp(&b.meta.title.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(entries)
}

pub fn delete_entry(entry: &Entry) -> Result<()> {
    if entry.mp3.exists() {
        fs::remove_file(&entry.mp3)
            .with_context(|| format!("удаление {}", entry.mp3.display()))?;
    }
    if entry.meta_path.exists() {
        fs::remove_file(&entry.meta_path)
            .with_context(|| format!("удаление {}", entry.meta_path.display()))?;
    }
    if entry.mp3.exists() || entry.meta_path.exists() {
        bail!("файлы всё ещё на месте после удаления");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ytl-lib-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_read_list_delete() {
        let dir = temp_dir();
        let meta = Meta {
            url: "https://youtu.be/abc123XYZ00".into(),
            title: "Test Track".into(),
        };
        write_meta(&dir, "abc123XYZ00", &meta).unwrap();
        fs::write(mp3_path(&dir, "abc123XYZ00"), b"fake").unwrap();

        let entries = list_entries(&dir).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "abc123XYZ00");
        assert_eq!(entries[0].meta.title, "Test Track");

        delete_entry(&entries[0]).unwrap();
        assert!(list_entries(&dir).unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
