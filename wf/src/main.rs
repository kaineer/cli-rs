use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let dir = wezterm_dir();

    match run(&dir, &args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("wf: {err}");
            ExitCode::FAILURE
        }
    }
}

fn wezterm_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/wezterm")
}

fn run(dir: &Path, args: &[String]) -> io::Result<()> {
    match args.first().map(String::as_str) {
        Some("lig" | "ligature") => {
            let rest: Vec<&str> = args[1..].iter().map(String::as_str).collect();
            set_ligatures(dir, &rest)
        }
        Some("ly") => set_ligatures_with_optional_font(dir, "yes", args.get(1)),
        Some("ln") => set_ligatures_with_optional_font(dir, "no", args.get(1)),
        Some("f" | "font") => set_font(dir, args.get(1).map(String::as_str).unwrap_or("")),
        Some("25") => set_font(dir, "large"),
        Some("16") => set_font(dir, "small"),
        Some("h" | "--help") => {
            usage();
            Ok(())
        }
        _ => {
            set_ligatures(dir, &["no"])?;
            set_font(dir, "small")
        }
    }
}

fn usage() {
    println!(" $ wf ly           # turn ligatures on");
    println!(" $ wf ln           # turn ligatures off");
    println!(" $ wf ly 25        # ligatures on, then font size 25");
    println!(" $ wf ln 16        # ligatures off, then font size 16");
    println!(" $ wf lig on       # turn ligatures on");
    println!(" $ wf lig off 25   # ligatures off, then font size 25");
    println!(" $ wf 25           # set font size 25");
    println!(" $ wf 16           # set font size 16");
    println!(" $ wf              # set font to 16, set ligatures off");
    println!();
    println!(" Ligature commands (ly/ln/lig) first set ligatures,");
    println!(" then, if another argument is given, set font size.");
}

fn set_ligatures_with_optional_font(
    dir: &Path,
    ligatures: &str,
    font: Option<&String>,
) -> io::Result<()> {
    match font.map(String::as_str) {
        Some(font) => set_ligatures(dir, &[ligatures, font]),
        None => set_ligatures(dir, &[ligatures]),
    }
}

fn set_ligatures(dir: &Path, args: &[&str]) -> io::Result<()> {
    match args.first().copied().unwrap_or("") {
        "yes" | "on" => copy_config(dir, "ligature.enabled.lua", "ligature.lua")?,
        "no" | "off" => copy_config(dir, "ligature.disabled.lua", "ligature.lua")?,
        _ => {}
    }

    if let Some(font) = args.get(1).copied().filter(|s| !s.is_empty()) {
        set_font(dir, font)?;
    }

    Ok(())
}

fn set_font(dir: &Path, size: &str) -> io::Result<()> {
    match size {
        "big" | "large" | "25" => copy_config(dir, "font.25.lua", "font.lua"),
        "small" | "16" => copy_config(dir, "font.16.lua", "font.lua"),
        _ => Ok(()),
    }
}

fn copy_config(dir: &Path, from: &str, to: &str) -> io::Result<()> {
    fs::copy(dir.join(from), dir.join(to))?;
    Ok(())
}
