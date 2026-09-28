//! `es status` — show scheme/input resolution without opening the editor.

use anyhow::{bail, Result};
use serde_yaml::{Mapping, Value};

use crate::data::{self, DataFormat};
use crate::io_args::IoArgs;
use crate::parse::{self, Schema};

pub fn run(args: IoArgs) -> Result<()> {
    if !args.scheme.is_file() {
        bail!("схема не найдена: {}", args.scheme.display());
    }

    let schema = parse::parse_file(&args.scheme)?;
    let loaded = data::load(&args.input)?;
    print_status(&args, &schema, &loaded.map, &loaded.format);
    Ok(())
}

fn print_status(args: &IoArgs, schema: &Schema, data: &Mapping, format: &DataFormat) {
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
    println!("  format:  {}", format_label(format));
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

fn format_label(format: &DataFormat) -> &'static str {
    match format {
        DataFormat::Yaml => "yaml",
        DataFormat::Shell { .. } => "shell (export)",
    }
}
