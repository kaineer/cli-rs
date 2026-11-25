//! ytl — скачать youtube-аудио через ytdl или выбрать трек в TUI.

mod fuzzy;
mod library;
mod play;
mod tui;
mod ytdl;

use anyhow::{bail, Result};

enum Mode {
    /// Интерактивный список библиотеки.
    Browse { debug: bool },
    /// Скачать URL; `play` — запустить mplayer после.
    Download { url: String, play: bool },
}

fn parse_args(args: &[String]) -> Result<Mode> {
    let mut no_play = false;
    let mut debug = false;
    let mut url: Option<String> = None;

    for arg in args {
        match arg.as_str() {
            "--no-play" => no_play = true,
            "--debug" => debug = true,
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            s if s.starts_with('-') => bail!("неизвестный флаг: {s}"),
            s => {
                if url.is_some() {
                    bail!("ожидался один URL, получили лишний аргумент: {s}");
                }
                url = Some(s.to_string());
            }
        }
    }

    match url {
        None if no_play => bail!("--no-play имеет смысл только вместе с URL"),
        None => Ok(Mode::Browse { debug }),
        Some(url) => Ok(Mode::Download {
            url,
            play: !no_play,
        }),
    }
}

fn print_usage() {
    eprintln!(
        "\
использование:
  ytl [--no-play] <youtube-url>   скачать mp3 в ~/Music/ytdl/
  ytl [--debug]                   TUI: поиск и воспроизведение

флаги:
  --no-play   не запускать mplayer после скачивания
  --debug     в TUI показать рамки, подписи и подсказки

окружение:
  YTDL_BIN    путь к ytdl, если его нет в PATH

TUI:
  /           открыть поиск (Esc — закрыть)
  j/k         вверх/вниз (когда поиск закрыт)
  enter       играть   x — удалить   q/esc — выход
"
    );
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args)? {
        Mode::Browse { debug } => tui::run(debug),
        Mode::Download { url, play } => download_and_maybe_play(&url, play),
    }
}

fn download_and_maybe_play(url: &str, play_after: bool) -> Result<()> {
    let bin = ytdl::resolve_bin()?;
    let dir = library::ensure_music_dir()?;

    eprintln!("ytdl: {}", bin.display());
    eprintln!("каталог: {}", dir.display());
    eprintln!("url: {url}");

    let id = ytdl::download(&bin, url, &dir)?;
    let mp3 = library::mp3_path(&dir, &id);
    let meta = library::read_meta(&library::meta_path(&dir, &id))?;

    eprintln!("готово: {}", mp3.display());
    eprintln!("title: {}", meta.title);

    if play_after {
        play::play_mp3(&mp3)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_when_no_args() {
        match parse_args(&[]).unwrap() {
            Mode::Browse { debug: false } => {}
            _ => panic!("ожидали Browse"),
        }
    }

    #[test]
    fn browse_debug() {
        match parse_args(&["--debug".into()]).unwrap() {
            Mode::Browse { debug: true } => {}
            _ => panic!("ожидали Browse с debug"),
        }
    }

    #[test]
    fn download_with_url() {
        match parse_args(&["https://youtu.be/abc".into()]).unwrap() {
            Mode::Download { url, play } => {
                assert_eq!(url, "https://youtu.be/abc");
                assert!(play);
            }
            _ => panic!("ожидали Download"),
        }
    }

    #[test]
    fn no_play_flag() {
        match parse_args(&["--no-play".into(), "https://youtu.be/abc".into()]).unwrap() {
            Mode::Download { play, .. } => assert!(!play),
            _ => panic!("ожидали Download"),
        }
        match parse_args(&["https://youtu.be/abc".into(), "--no-play".into()]).unwrap() {
            Mode::Download { play, .. } => assert!(!play),
            _ => panic!("ожидали Download"),
        }
    }

    #[test]
    fn no_play_alone_is_error() {
        assert!(parse_args(&["--no-play".into()]).is_err());
    }
}
