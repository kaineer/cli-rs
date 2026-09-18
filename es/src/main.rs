// es — YAML schema parser / TUI editor (WIP)

mod dump;
mod edit;
mod io_args;
mod kitchensink;
mod parse;
mod status;
mod widgets;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Result};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        bail!("{}", usage());
    };

    match cmd.as_str() {
        "kitchensink" | "ks" => {
            if args.next().is_some() {
                bail!("{}", usage());
            }
            kitchensink::run()
        }
        "status" => {
            let io = io_args::parse_args(args)?;
            status::run(io)
        }
        "edit" => {
            let io = io_args::parse_args(args)?;
            edit::run(io)
        }
        "dump" => {
            let Some(path) = args.next() else {
                bail!("{}", usage());
            };
            if args.next().is_some() {
                bail!("{}", usage());
            }
            dump_schema(PathBuf::from(path))
        }
        path => {
            if args.next().is_some() {
                bail!("{}", usage());
            }
            dump_schema(PathBuf::from(path))
        }
    }
}

fn dump_schema(path: PathBuf) -> Result<()> {
    if !path.is_file() {
        bail!("файл не найден: {}", path.display());
    }
    let schema = crate::parse::parse_file(&path)?;
    dump::dump(&schema);
    Ok(())
}

fn usage() -> String {
    format!(
        "usage:\n  es <schema.yaml>\n  es dump <schema.yaml>\n  es status --scheme <s.yaml> --input <d.yaml> [--output <o.yaml>]\n  es edit --scheme <s.yaml> --input <d.yaml> [--output <o.yaml>]\n  es kitchensink"
    )
}
