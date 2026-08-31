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

    /// Create backup of original file (default: true)
    #[arg(long, default_value_t = true)]
    backup: bool,

    /// Don't create backup of original file
    #[arg(long)]
    no_backup: bool,

    /// Dry run - show what would be changed without writing files
    #[arg(short, long)]
    dry_run: bool,

    /// Output directory for CSS files (default: same as source)
    #[arg(short, long)]
    output_dir: Option<PathBuf>,

    /// Quiet mode - no output (default)
    #[arg(short, long, conflicts_with = "verbose")]
    quiet: bool,

    /// Verbose mode - show all output
    #[arg(short, long, conflicts_with = "quiet")]
    verbose: bool,
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

/// Структура для управления выводом
struct Logger {
    quiet: bool,
    verbose: bool,
}

impl Logger {
    fn new(quiet: bool, verbose: bool) -> Self {
        Self { quiet, verbose }
    }

    fn log(&self, message: &str) {
        if !self.quiet {
            println!("{}", message);
        }
    }

    fn log_verbose(&self, message: &str) {
        if self.verbose && !self.quiet {
            println!("{}", message);
        }
    }

    fn log_success(&self, message: &str) {
        if !self.quiet {
            println!("{} {}", "✅".green(), message);
        }
    }

    fn log_info(&self, message: &str) {
        if !self.quiet {
            println!("{} {}", "ℹ️".blue(), message);
        }
    }

    fn log_warning(&self, message: &str) {
        if !self.quiet {
            println!("{} {}", "⚠️".yellow(), message);
        }
    }

    fn log_error(&self, message: &str) {
        if !self.quiet {
            println!("{} {}", "❌".red(), message);
        }
    }

    fn log_debug(&self, message: &str) {
        if self.verbose && !self.quiet {
            println!("{} {}", "🔍".dimmed(), message);
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let logger = Logger::new(cli.quiet, cli.verbose);

    // Правильная логика: backup включен только если указан --backup И НЕ указан --no-backup
    let should_backup = cli.backup && !cli.no_backup;

    // Если не quiet и не verbose, показываем базовую информацию
    if !cli.quiet && !cli.verbose {
        logger.log(&format!("{}", "🚀 Inline Style to CSS Module Converter".bold().cyan()));
        logger.log(&format!("{} {}", "📖 Processing:".dimmed(), cli.file.display().to_string().yellow()));
        logger.log("");
    }

    // Если verbose, показываем расширенную информацию
    if cli.verbose {
        logger.log_verbose(&format!("🔍 File: {}", cli.file.display()));
        logger.log_verbose(&format!("🔍 Backup: {}", should_backup));
        logger.log_verbose(&format!("🔍 Dry run: {}", cli.dry_run));
        if let Some(dir) = &cli.output_dir {
            logger.log_verbose(&format!("🔍 Output dir: {}", dir.display()));
        }
        logger.log_verbose("");
    }

    // Parse the file
    let source = std::fs::read_to_string(&cli.file)?;
    let program = parser::parse_jsx(&source, &cli.file)?;

    // Extract styles
    let style_mappings = parser::extract_styles(&program, &source)?;

    if style_mappings.is_empty() {
        logger.log_info("No inline styles found");
        return Ok(());
    }

    let total = style_mappings.len();
    let const_only = style_mappings
        .iter()
        .filter(|m| m.dynamic_props.is_empty())
        .count();
    let mixed = total - const_only;

    logger.log(&format!(
        "{} Found {} styles with constant properties",
        "🔍".bold(),
        total.to_string().yellow()
    ));
    logger.log(&format!("   • Fully constant: {}", const_only.to_string().green()));
    logger.log(&format!("   • Mixed (with dynamic): {}", mixed.to_string().yellow()));
    logger.log("");

    if mixed > 0 && cli.verbose {
        logger.log_verbose("Mixed styles (constants + dynamic):");
        for mapping in style_mappings.iter().filter(|m| !m.dynamic_props.is_empty()) {
            let const_list: Vec<String> = mapping.const_props.keys().cloned().collect();
            let dyn_list: Vec<String> = mapping.dynamic_props.keys().cloned().collect();
            logger.log_verbose(&format!(
                "   • <{}>: constants: {} | dynamic: {}",
                mapping.tag_name.green(),
                const_list.join(", "),
                dyn_list.join(", ")
            ));
        }
        logger.log_verbose("");
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

    let css_content_display = if cli.verbose { Some(css_content.clone()) } else { None };

    if !cli.dry_run {
        std::fs::write(&css_path, &css_content)?;
        logger.log_success(&format!("Created CSS file: {}", css_path.display().to_string().yellow()));
        logger.log(&format!("   • Rules created: {}", style_mappings.len()));
        
        if let Some(content) = css_content_display {
            logger.log_verbose(&format!("📄 CSS content:\n{}", content));
        }
    } else {
        logger.log_info(&format!("Would create CSS file: {}", css_path.display()));
        if let Some(content) = css_content_display {
            logger.log_verbose(&format!("📄 CSS content:\n{}", content));
        }
    }

    // Transform the source code
    let transformed = transformer::transform_jsx(&source, &style_mappings)?;
    let transformed = transformer::add_css_import(&transformed, &cli.file)?;

    let transformed_display = if cli.verbose { Some(transformed.clone()) } else { None };

    if !cli.dry_run {
        // Backup original (только если should_backup == true)
        if should_backup {
            let original_ext = cli.file.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("tsx");
            
            let backup_name = format!(
                "{}.{}.bak",
                cli.file.file_stem().unwrap().to_str().unwrap(),
                original_ext
            );
            let backup_path = cli.file.parent().unwrap_or(PathBuf::from(".").as_path()).join(backup_name);
            
            std::fs::copy(&cli.file, &backup_path)?;
            logger.log_success(&format!("Backup saved: {}", backup_path.display().to_string().dimmed()));
        } else {
            logger.log_verbose("⏭️ Skipping backup (disabled)");
        }

        // Write transformed file
        std::fs::write(&cli.file, &transformed)?;
        logger.log_success(&format!("File updated: {}", cli.file.display().to_string().yellow()));
        
        if let Some(content) = transformed_display {
            logger.log_verbose(&format!("📄 Transformed content:\n{}", content));
        }
    } else {
        logger.log_info(&format!("Would transform file: {}", cli.file.display()));
        if let Some(content) = transformed_display {
            logger.log_verbose(&format!("📄 Transformed content:\n{}", content));
        }
    }

    if mixed > 0 && !cli.dry_run && !cli.quiet {
        logger.log("");
        logger.log_warning("Recommendation:");
        logger.log_warning("   Check files with mixed styles.");
        logger.log_warning("   Constant properties moved to CSS, dynamic remain in style.");
    }

    Ok(())
}
