//! `es status` — show scheme/input resolution without opening the editor.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_yaml::{Mapping, Value};

use crate::io_args::IoArgs;
use crate::parse::{self, Schema};

pub fn run(args: IoArgs) -> Result<()> {
    if !args.scheme.is_file() {
        bail!("схема не найдена: {}", args.scheme.display());
    }

    let schema = parse::parse_file(&args.scheme)?;
    let data = load_input(&args.input)?;
    print_status(&args, &schema, &data);
    Ok(())
}

fn load_input(path: &Path) -> Result<Mapping> {
    if !path.exists() {
        return Ok(Mapping::new());
    }
    if !path.is_file() {
        bail!("input не файл: {}", path.display());
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("не удалось прочитать {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(Mapping::new());
    }
    let root: Value = serde_yaml::from_str(&text)
        .with_context(|| format!("невалидный YAML в {}", path.display()))?;
    match root {
        Value::Mapping(m) => Ok(m),
        Value::Null => Ok(Mapping::new()),
        other => bail!(
            "корень input должен быть YAML-мапой, получено {}",
            type_name(&other)
        ),
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

fn print_status(args: &IoArgs, schema: &Schema, data: &Mapping) {
    println!("status");
    println!("  scheme:  {}", args.scheme.display());
    println!("  input:   {}", args.input.display());
    println!(
        "  output:  {}{}",
        args.output.display(),
        if args.output == args.input {
            " (= input)"
        } else {
            ""
        }
    );
    println!(
        "  fields:  {} (skipped: {})",
        schema.fields.len(),
        schema.skipped.len()
    );
    println!("  data keys: {}", data.len());
    for f in &schema.fields {
        let present = data.contains_key(Value::String(f.key.clone()));
        println!(
            "    - {} {}",
            f.key,
            if present { "(in input)" } else { "(missing)" }
        );
    }
}
