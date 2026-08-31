mod parser;
mod transformer;
mod css_generator;

use std::path::PathBuf;
use clap::Parser;
use colored::*;
use anyhow::Result;

#[derive(Parser, Debug)]
#[command(author, version, about = "Migrates inline styles to CSS modules")]
struct Cli {
    /// Path to the React component file (.tsx or .jsx)
    #[arg(value_parser = validate_file)]
    file: PathBuf,

    /// Create backup of original file
    #[arg(short, long, default_value_t = true)]
    backup: bool,

    /// Dry run - show what would be changed
    #[arg(short, long)]
    dry_run: bool,

    /// Output directory for CSS files (default: same as source)
    #[arg(short, long)]
    output_dir: Option<PathBuf>,
}

fn validate_file(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(format!("File does not exist: {}", path.display()));
    }
    if let Some(ext) = path.extension() {
        if ext != "tsx" && ext != "jsx" && ext != "ts" && ext != "js" {
            return Err(format!("Unsupported file extension: {:?}", ext));
        }
    }
    Ok(path)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("{}", "🚀 Inline Style to CSS Module Converter".bold().cyan());
    println!(
        "{} {}",
        "📖 Processing:".dimmed(),
        cli.file.display().to_string().yellow()
    );
    println!();

    // Parse the file
    let source = std::fs::read_to_string(&cli.file)?;
    let program = parser::parse_jsx(&source, &cli.file)?;

    // Extract styles
    let style_mappings = parser::extract_styles(&program, &source)?;

    if style_mappings.is_empty() {
        println!("{} No inline styles found", "ℹ️".blue());
        return Ok(());
    }

    let total = style_mappings.len();
    let const_only = style_mappings
        .iter()
        .filter(|m| m.dynamic_props.is_empty())
        .count();
    let mixed = total - const_only;

    println!(
        "{} Found {} styles with constant properties",
        "🔍".bold(),
        total.to_string().yellow()
    );
    println!("   • Fully constant: {}", const_only.to_string().green());
    println!("   • Mixed (with dynamic): {}", mixed.to_string().yellow());
    println!();

    if mixed > 0 {
        println!("{} Mixed styles (constants + dynamic):", "⚠️".yellow());
        for mapping in style_mappings.iter().filter(|m| !m.dynamic_props.is_empty()) {
            let const_list: Vec<String> = mapping.const_props.keys().cloned().collect();
            let dyn_list: Vec<String> = mapping.dynamic_props.keys().cloned().collect();
            println!(
                "   • <{}>: constants: {} | dynamic: {}",
                mapping.tag_name.green(),
                const_list.join(", "),
                dyn_list.join(", ")
            );
        }
        println!();
    }

    // Generate CSS
    let css_content = css_generator::generate_css(&style_mappings)?;
    let css_filename = format!(
        "{}.module.css",
        cli.file.file_stem().unwrap().to_str().unwrap()
    );

    let default_path = PathBuf::from(".");
    let css_path = cli
        .output_dir
        .clone()
        .unwrap_or_else(|| {
            cli.file
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or(default_path)
        })
        .join(css_filename);

    if !cli.dry_run {
        std::fs::write(&css_path, css_content)?;
        println!(
            "{} Created CSS file: {}",
            "✅".green(),
            css_path.display().to_string().yellow()
        );
        println!("   • Rules created: {}", style_mappings.len());
    } else {
        println!(
            "{} Would create CSS file: {}",
            "🔍".dimmed(),
            css_path.display()
        );
        println!("{}", css_content.dimmed());
    }

    // Transform the source code
    let transformed = transformer::transform_jsx(&source, &style_mappings)?;
    let transformed = transformer::add_css_import(&transformed, &cli.file)?;

    if !cli.dry_run {
        // Backup original - сохраняем с правильным расширением
        if cli.backup {
            // Получаем расширение оригинального файла
            let original_ext = cli.file.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("tsx");
            
            // Создаем имя бэкапа с правильным расширением
            let backup_name = format!(
                "{}.{}.bak",
                cli.file.file_stem().unwrap().to_str().unwrap(),
                original_ext
            );
            let backup_path = cli.file.parent().unwrap_or(PathBuf::from(".").as_path()).join(backup_name);
            
            std::fs::copy(&cli.file, &backup_path)?;
            println!(
                "💾 Backup saved: {}",
                backup_path.display().to_string().dimmed()
            );
        }

        // Write transformed file
        std::fs::write(&cli.file, transformed)?;
        println!(
            "{} File updated: {}",
            "✅".green(),
            cli.file.display().to_string().yellow()
        );
    } else {
        println!("{} Would transform file:", "🔍".dimmed());
        println!("{}", transformed.dimmed());
    }

    if mixed > 0 && !cli.dry_run {
        println!();
        println!("{} Recommendation:", "💡".yellow());
        println!("   Check files with mixed styles.");
        println!("   Constant properties moved to CSS, dynamic remain in style.");
    }

    Ok(())
}
