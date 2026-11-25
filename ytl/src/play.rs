//! Проигрывание через `mplayer`.

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

pub fn play_mp3(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("нет файла {}", path.display());
    }

    // Код возврата mplayer при 'q' часто ненулевой — это нормально.
    let _status = Command::new("mplayer")
        .arg(path)
        .status()
        .context("запуск mplayer (нужен в PATH)")?;
    Ok(())
}
