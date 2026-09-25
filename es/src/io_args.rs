//! Shared `--scheme` / `--input` / `--output` options.

use std::path::PathBuf;

use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub struct IoArgs {
    pub scheme: PathBuf,
    pub input: PathBuf,
    pub output: PathBuf,
    /// Hide the navigation help header in `edit`.
    pub silent: bool,
}

/// Parse options: `--scheme`, `--input`, optional `--output` (defaults to input),
/// optional `--silent` (edit only).
pub fn parse_args(mut args: impl Iterator<Item = String>) -> Result<IoArgs> {
    let mut scheme: Option<PathBuf> = None;
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut silent = false;

    while let Some(arg) = args.next() {
        if arg == "--silent" {
            if silent {
                bail!("повторный --silent");
            }
            silent = true;
            continue;
        }

        let (flag, value) = split_flag(&arg, &mut args)?;
        match flag.as_str() {
            "--scheme" => {
                if scheme.is_some() {
                    bail!("повторный --scheme");
                }
                scheme = Some(PathBuf::from(value));
            }
            "--input" => {
                if input.is_some() {
                    bail!("повторный --input");
                }
                input = Some(PathBuf::from(value));
            }
            "--output" => {
                if output.is_some() {
                    bail!("повторный --output");
                }
                output = Some(PathBuf::from(value));
            }
            other => bail!("неизвестная опция: {other}"),
        }
    }

    let Some(scheme) = scheme else {
        bail!("нужен --scheme <path>\n{}", usage_io());
    };
    let Some(input) = input else {
        bail!("нужен --input <path>\n{}", usage_io());
    };
    let output = output.unwrap_or_else(|| input.clone());

    Ok(IoArgs {
        scheme,
        input,
        output,
        silent,
    })
}

fn split_flag(
    arg: &str,
    rest: &mut impl Iterator<Item = String>,
) -> Result<(String, String)> {
    if let Some((flag, value)) = arg.split_once('=') {
        if !flag.starts_with("--") || value.is_empty() {
            bail!("некорректный аргумент: {arg}");
        }
        return Ok((flag.to_string(), value.to_string()));
    }
    if !arg.starts_with("--") {
        bail!("неожиданный аргумент: {arg} (ожидались опции --scheme/--input/--output/--silent)");
    }
    let Some(value) = rest.next() else {
        bail!("опция {arg} требует значение");
    };
    Ok((arg.to_string(), value))
}

pub fn usage_io() -> &'static str {
    "  es <cmd> --scheme <schema.yaml> --input <data.yaml> [--output <out.yaml>] [--silent]"
}
