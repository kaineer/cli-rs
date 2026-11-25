//! Обёртка над бинарником `ytdl` (yt-dlp).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::library::{self, Meta};

/// Путь к ytdl: `$YTDL_BIN` или `which ytdl`.
pub fn resolve_bin() -> Result<PathBuf> {
    if let Some(from_env) = std::env::var_os("YTDL_BIN") {
        let path = PathBuf::from(from_env);
        if path.is_file() {
            return Ok(path);
        }
        bail!(
            "YTDL_BIN указывает на несуществующий файл: {}",
            path.display()
        );
    }

    which("ytdl").context(
        "ytdl не найден в PATH; установите его или задайте YTDL_BIN=/path/to/ytdl",
    )
}

fn which(name: &str) -> Result<PathBuf> {
    let output = Command::new("which")
        .arg(name)
        .output()
        .with_context(|| format!("запуск which {name}"))?;
    if !output.status.success() {
        bail!("which {name}: не найден");
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        bail!("which {name}: пустой ответ");
    }
    Ok(PathBuf::from(path))
}

/// Скачивает mp3 в `dir`, пишет `<id>.mp3.meta`, возвращает id.
///
/// Прогресс ytdl идёт в stderr терминала; id/title читаем из stdout
/// (`--print after_move:…`).
pub fn download(bin: &Path, url: &str, dir: &Path) -> Result<String> {
    library::ensure_music_dir()?;

    let template = dir.join("%(id)s.%(ext)s");
    let mut child = Command::new(bin)
        .arg("-x")
        .arg("--audio-format")
        .arg("mp3")
        .arg("-o")
        .arg(&template)
        .arg("--print")
        .arg("after_move:%(id)s")
        .arg("--print")
        .arg("after_move:%(title)s")
        .arg("--no-playlist")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("запуск {}", bin.display()))?;

    let mut stdout = String::new();
    {
        let mut pipe = child
            .stdout
            .take()
            .context("не удалось открыть stdout ytdl")?;
        pipe.read_to_string(&mut stdout)
            .context("чтение stdout ytdl")?;
    }

    let status = child.wait().context("ожидание ytdl")?;
    if !status.success() {
        bail!("ytdl завершился с ошибкой ({status})");
    }

    let mut lines = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with("WARNING:") && !l.starts_with("ERROR:"));

    let id = lines
        .next()
        .context("ytdl не вернул id после скачивания")?
        .to_string();
    let title = lines
        .next()
        .context("ytdl не вернул title после скачивания")?
        .to_string();

    let mp3 = library::mp3_path(dir, &id);
    if !mp3.is_file() {
        bail!("ожидали файл {}, но его нет", mp3.display());
    }

    library::write_meta(
        dir,
        &id,
        &Meta {
            url: url.to_string(),
            title,
        },
    )?;

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_bin_finds_something_or_errors_cleanly() {
        // В CI/песочнице ytdl может отсутствовать — проверяем только, что функция не паникует.
        let _ = resolve_bin();
    }
}
