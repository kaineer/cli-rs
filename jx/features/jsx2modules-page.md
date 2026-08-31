Конечно! Сделаю полноценную реализацию на Rust с использованием `oxc_parser`. Это будет быстрый, безопасный и надежный инструмент для миграции инлайн-стилей в CSS-модули.

## Полная реализация на Rust

### 1. Создаем проект

```bash
cargo new inline-to-css-module --bin
cd inline-to-css-module
```

### 2. `Cargo.toml`

```toml
[package]
name = "inline-to-css-module"
version = "0.1.0"
edition = "2021"

[dependencies]
oxc = { version = "0.36.0", features = ["parser", "span", "allocator"] }
anyhow = "1.0"
walkdir = "2.5"
colored = "2.1"
regex = "1.10"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
glob = "0.3"

[dev-dependencies]
pretty_assertions = "1.4"
```

### 3. Основной код `src/main.rs`

```rust
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
    println!("{} {}", "📖 Processing:".dimmed(), cli.file.display().to_string().yellow());
    println!();

    // Parse the file
    let source = std::fs::read_to_string(&cli.file)?;
    let parsed = parser::parse_jsx(&source, &cli.file)?;

    // Extract styles
    let style_mappings = parser::extract_styles(&parsed, &source)?;
    
    if style_mappings.is_empty() {
        println!("{} No inline styles found", "ℹ️".blue());
        return Ok(());
    }

    let total = style_mappings.len();
    let const_only = style_mappings.iter().filter(|m| m.dynamic_props.is_empty()).count();
    let mixed = total - const_only;

    println!("{} Found {} styles with constant properties", "🔍".bold(), total.to_string().yellow());
    println!("   • Fully constant: {}", const_only.to_string().green());
    println!("   • Mixed (with dynamic): {}", mixed.to_string().yellow());
    println!();

    if mixed > 0 {
        println!("{} Mixed styles (constants + dynamic):", "⚠️".yellow());
        for mapping in style_mappings.iter().filter(|m| !m.dynamic_props.is_empty()) {
            println!(
                "   • <{}>: constants: {} | dynamic: {}",
                mapping.tag_name.green(),
                mapping.const_props.keys().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
                mapping.dynamic_props.keys().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            );
        }
        println!();
    }

    // Generate CSS
    let css_content = css_generator::generate_css(&style_mappings)?;
    let css_path = cli.output_dir
        .clone()
        .unwrap_or_else(|| cli.file.parent().unwrap_or(PathBuf::from(".")).to_path_buf())
        .join(format!("{}.module.css", cli.file.file_stem().unwrap().to_str().unwrap()));

    if !cli.dry_run {
        std::fs::write(&css_path, css_content)?;
        println!("{} Created CSS file: {}", "✅".green(), css_path.display().to_string().yellow());
        println!("   • Rules created: {}", style_mappings.len());
    } else {
        println!("{} Would create CSS file: {}", "🔍".dimmed(), css_path.display());
        println!("{}", css_content.dimmed());
    }

    // Transform the source code
    let transformed = transformer::transform_jsx(&source, &style_mappings)?;
    let transformed = transformer::add_css_import(&transformed, &cli.file)?;

    if !cli.dry_run {
        // Backup original
        if cli.backup {
            let backup_path = cli.file.with_extension("tsx.bak");
            std::fs::copy(&cli.file, &backup_path)?;
            println!("💾 Backup saved: {}", backup_path.display().to_string().dimmed());
        }

        // Write transformed file
        std::fs::write(&cli.file, transformed)?;
        println!("{} File updated: {}", "✅".green(), cli.file.display().to_string().yellow());
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
```

### 4. Парсер `src/parser.rs`

```rust
use oxc::allocator::Allocator;
use oxc::parser::Parser;
use oxc::span::SourceType;
use oxc::ast::ast::*;
use oxc::ast::visit::{walk, Visit};
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct StyleMapping {
    pub tag_name: String,
    pub const_props: HashMap<String, String>,
    pub dynamic_props: HashMap<String, bool>,
    pub start: usize,
    pub end: usize,
}

/// Парсит JSX/TSX файл в AST
pub fn parse_jsx(source: &str, path: &Path) -> Result<Program<'static>> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_else(|| {
        if path.extension().map(|e| e == "tsx").unwrap_or(false) {
            SourceType::tsx()
        } else {
            SourceType::jsx()
        }
    });

    let parser = Parser::new(&allocator, source, source_type);
    let program = parser.parse();

    if !program.errors.is_empty() {
        let errors: Vec<String> = program.errors
            .iter()
            .map(|e| format!("{:?}", e))
            .collect();
        return Err(anyhow::anyhow!("Parse errors: {}", errors.join("\n")));
    }

    Ok(program.program)
}

/// Visitor для извлечения стилей из AST
pub struct StyleExtractor {
    pub mappings: Vec<StyleMapping>,
    source: String,
}

impl StyleExtractor {
    pub fn new(source: &str) -> Self {
        Self {
            mappings: Vec::new(),
            source: source.to_string(),
        }
    }

    /// Проверяет, является ли узел динамическим
    fn is_dynamic_node(&self, expr: &Expression) -> bool {
        match expr {
            Expression::Identifier(_) => true,
            Expression::MemberExpression(_) => true,
            Expression::CallExpression(_) => true,
            Expression::ConditionalExpression(_) => true,
            Expression::LogicalExpression(_) => true,
            Expression::BinaryExpression(_) => true,
            Expression::UnaryExpression(_) => true,
            Expression::TemplateLiteral(_) => true,
            Expression::ArrowFunctionExpression(_) => true,
            Expression::FunctionExpression(_) => true,
            _ => false,
        }
    }

    /// Извлекает свойства из объектного выражения
    fn extract_properties(&self, expr: &ObjectExpression) -> (HashMap<String, String>, HashMap<String, bool>) {
        let mut const_props = HashMap::new();
        let mut dynamic_props = HashMap::new();

        for prop in &expr.properties {
            if let ObjectPropertyKind::ObjectProperty(prop) = prop {
                if let PropertyKind::Property(prop) = &prop.kind {
                    if let Some(prop_name) = &prop.key {
                        if let Expression::Identifier(ident) = prop_name.as_ref() {
                            let key = ident.name.to_string();
                            
                            if let Some(value_expr) = &prop.value {
                                if self.is_dynamic_node(value_expr.as_ref()) {
                                    dynamic_props.insert(key, true);
                                } else {
                                    // Извлекаем константное значение
                                    if let Expression::Literal(lit) = value_expr.as_ref() {
                                        let value = match lit {
                                            Literal::String(s) => s.to_string(),
                                            Literal::Number(n) => {
                                                if key == "opacity" || key == "zIndex" || key == "flex" {
                                                    n.to_string()
                                                } else {
                                                    format!("{}px", n)
                                                }
                                            },
                                            Literal::Boolean(b) => b.to_string(),
                                            _ => continue,
                                        };
                                        const_props.insert(key, value);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        (const_props, dynamic_props)
    }

    /// Рекурсивно обходит AST и извлекает стили
    pub fn extract_styles(&mut self, program: &Program) {
        self.visit_program(program);
    }
}

impl Visit<'static> for StyleExtractor {
    fn visit_jsx_element(&mut self, element: &JSXElement<'static>) {
        // Получаем имя тега
        let tag_name = match &element.opening_element.name {
            JSXElementName::Identifier(ident) => ident.name.to_string().to_lowercase(),
            _ => "div".to_string(),
        };

        // Ищем атрибут style
        for attr in &element.opening_element.attributes {
            if let JSXAttributeItem::Attribute(attr) = attr {
                if let JSXAttributeName::Identifier(ident) = &attr.name {
                    if ident.name == "style" {
                        if let Some(attr_value) = &attr.value {
                            if let JSXAttributeValue::ExpressionContainer(container) = attr_value {
                                if let Some(expr) = &container.expression {
                                    if let Expression::ObjectExpression(obj_expr) = expr.as_ref() {
                                        let (const_props, dynamic_props) = self.extract_properties(obj_expr);
                                        
                                        if !const_props.is_empty() {
                                            let span = attr.span;
                                            self.mappings.push(StyleMapping {
                                                tag_name: tag_name.clone(),
                                                const_props,
                                                dynamic_props,
                                                start: span.start as usize,
                                                end: span.end as usize,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Продолжаем обход
        walk::walk_jsx_element(self, element);
    }
}

/// Основная функция для извлечения стилей
pub fn extract_styles(program: &Program, source: &str) -> Result<Vec<StyleMapping>> {
    let mut extractor = StyleExtractor::new(source);
    extractor.extract_styles(program);
    Ok(extractor.mappings)
}
```

### 5. Трансформер `src/transformer.rs`

```rust
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use regex::Regex;

use crate::parser::StyleMapping;

/// Трансформирует JSX код, заменяя стили на className + style
pub fn transform_jsx(source: &str, mappings: &[StyleMapping]) -> Result<String> {
    let mut result = source.to_string();
    let mut tag_counter: HashMap<String, usize> = HashMap::new();

    // Обрабатываем в обратном порядке, чтобы не сбивать индексы
    for mapping in mappings.iter().rev() {
        let rule_name = generate_rule_name(&mapping.tag_name, &mut tag_counter);
        
        // Находим атрибут style
        let context_start = mapping.start.saturating_sub(300);
        let context_end = (mapping.end + 300).min(result.len());
        let context = &result[context_start..context_end];

        let style_pattern = Regex::new(r"style\s*=\s*\{[^}]*\}")?;
        if let Some(style_match) = style_pattern.find(context) {
            let attr_start = context_start + style_match.start();
            let attr_end = context_start + style_match.end();

            let style_content = &result[attr_start..attr_end];

            if !mapping.dynamic_props.is_empty() {
                // Смешанный случай: оставляем динамику, убираем константы
                let new_style = remove_const_props(style_content, &mapping.const_props)?;
                
                let replacement = if new_style.trim() == "style={}" {
                    format!("className={{classes.{}}}", rule_name)
                } else {
                    format!("{} className={{classes.{}}}", new_style, rule_name)
                };

                result.replace_range(attr_start..attr_end, &replacement);
            } else {
                // Полностью константный стиль
                let replacement = format!("className={{classes.{}}}", rule_name);
                result.replace_range(attr_start..attr_end, &replacement);
            }
        }
    }

    Ok(result)
}

/// Удаляет константные свойства из объекта style
fn remove_const_props(style_content: &str, const_props: &HashMap<String, String>) -> Result<String> {
    let mut result = style_content.to_string();

    for prop in const_props.keys() {
        // Удаляем свойства вида prop: value,
        let patterns = [
            format!(r"{}\s*:\s*[^,}}]+[,}}]", prop),
            format!(r"{}\s*:\s*['\"][^'\"]+['\"][,}}]", prop),
        ];

        for pattern in patterns {
            let re = Regex::new(&pattern)?;
            result = re.replace_all(&result, "").to_string();
        }
    }

    // Очищаем лишние запятые
    let result = result
        .replace(", }", "}")
        .replace("{ ,", "{")
        .replace(", ,", ",")
        .replace("{}", "");

    Ok(result)
}

/// Генерирует имя CSS правила на основе тега
fn generate_rule_name(tag: &str, counter: &mut HashMap<String, usize>) -> String {
    let count = counter.entry(tag.to_string()).or_insert(0);
    *count += 1;
    format!("{}{}", tag, count)
}

/// Добавляет импорт CSS модуля
pub fn add_css_import(source: &str, path: &Path) -> Result<String> {
    let import_stmt = format!(
        "import classes from \"./{}.module.css\";",
        path.file_stem()
            .map(|s| s.to_str().unwrap_or("styles"))
            .unwrap_or("styles")
    );

    // Проверяем, есть ли уже импорт
    if source.contains("module.css") {
        return Ok(source.to_string());
    }

    let lines: Vec<&str> = source.lines().collect();
    let mut result = Vec::new();
    let mut inserted = false;
    let mut import_index = 0;

    // Находим место для вставки (после React импортов)
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("import") && (line.contains("React") || line.contains("react")) {
            import_index = i + 1;
        } else if line.starts_with("import") && import_index > 0 {
            import_index = i + 1;
        } else if line.starts_with("import") && i < 10 {
            import_index = i + 1;
        }
    }

    // Вставляем импорт
    for (i, line) in lines.iter().enumerate() {
        if i == import_index && !inserted {
            result.push(&import_stmt);
            inserted = true;
        }
        result.push(line);
    }

    if !inserted {
        result.insert(0, &import_stmt);
    }

    Ok(result.join("\n"))
}
```

### 6. Генератор CSS `src/css_generator.rs`

```rust
use anyhow::Result;
use std::collections::HashMap;
use regex::Regex;

use crate::parser::StyleMapping;

/// Генерирует содержимое CSS файла
pub fn generate_css(mappings: &[StyleMapping]) -> Result<String> {
    if mappings.is_empty() {
        return Ok(String::new());
    }

    let mut css_lines = vec![
        "/* Автоматически сгенерировано из инлайн-стилей */".to_string(),
        "/* Константные стили вынесены из JSX */".to_string(),
        "".to_string(),
    ];

    let mut tag_counter: HashMap<String, usize> = HashMap::new();

    for mapping in mappings {
        if mapping.const_props.is_empty() {
            continue;
        }

        let count = tag_counter.entry(mapping.tag_name.clone()).or_insert(0);
        *count += 1;
        let rule_name = format!("{}{}", mapping.tag_name, count);

        css_lines.push(format!(".{} {{", rule_name));
        
        // Сортируем свойства для стабильности
        let mut props: Vec<(&String, &String)> = mapping.const_props.iter().collect();
        props.sort_by_key(|(k, _)| *k);

        for (prop, value) in props {
            let kebab_prop = camel_to_kebab(prop);
            css_lines.push(format!("  {}: {};", kebab_prop, value));
        }
        
        css_lines.push("}".to_string());
        css_lines.push("".to_string());
    }

    Ok(css_lines.join("\n"))
}

/// Конвертирует camelCase в kebab-case
fn camel_to_kebab(s: &str) -> String {
    let re = Regex::new(r"([a-z0-9])([A-Z])").unwrap();
    re.replace_all(s, |caps: &regex::Captures| {
        format!("{}-{}", &caps[1], &caps[2].to_lowercase())
    }).to_string().to_lowercase()
}
```

### 7. Тесты `tests/test_converter.rs`

```rust
use inline_to_css_module::{parser, transformer, css_generator};

#[test]
fn test_extract_const_style() {
    let source = r#"
        <div style={{ fontSize: '16px', color: 'blue' }}>
            Hello
        </div>
    "#;
    
    let parsed = parser::parse_jsx(source, "test.tsx").unwrap();
    let styles = parser::extract_styles(&parsed, source).unwrap();
    
    assert_eq!(styles.len(), 1);
    assert_eq!(styles[0].tag_name, "div");
    assert_eq!(styles[0].const_props.len(), 2);
    assert!(styles[0].dynamic_props.is_empty());
}

#[test]
fn test_extract_mixed_style() {
    let source = r#"
        <div style={{ 
            color: name == "" ? "blue" : "red",
            fontSize: '16px',
            padding: '10px'
        }}>
            Hello
        </div>
    "#;
    
    let parsed = parser::parse_jsx(source, "test.tsx").unwrap();
    let styles = parser::extract_styles(&parsed, source).unwrap();
    
    assert_eq!(styles.len(), 1);
    assert_eq!(styles[0].tag_name, "div");
    assert!(!styles[0].dynamic_props.is_empty());
    assert!(styles[0].dynamic_props.contains_key("color"));
}

#[test]
fn test_transform_jsx() {
    let source = r#"
        <div style={{ fontSize: '16px', color: 'blue' }}>
            Hello
        </div>
    "#;
    
    let parsed = parser::parse_jsx(source, "test.tsx").unwrap();
    let styles = parser::extract_styles(&parsed, source).unwrap();
    let transformed = transformer::transform_jsx(source, &styles).unwrap();
    
    assert!(transformed.contains("className={classes.div1}"));
    assert!(!transformed.contains("style="));
}

#[test]
fn test_generate_css() {
    let source = r#"
        <div style={{ fontSize: '16px', color: 'blue' }}>
            Hello
        </div>
        <span style={{ fontWeight: 'bold' }}>
            World
        </span>
    "#;
    
    let parsed = parser::parse_jsx(source, "test.tsx").unwrap();
    let styles = parser::extract_styles(&parsed, source).unwrap();
    let css = css_generator::generate_css(&styles).unwrap();
    
    assert!(css.contains(".div1 {"));
    assert!(css.contains("font-size: 16px;"));
    assert!(css.contains("color: blue;"));
    assert!(css.contains(".span1 {"));
    assert!(css.contains("font-weight: bold;"));
}
```

### 8. Сборка и установка

```bash
# Сборка релизной версии
cargo build --release

# Запуск на файле
./target/release/inline-to-css-module ./src/components/MyComponent.tsx

# Запуск с бэкапом
./target/release/inline-to-css-module ./src/components/MyComponent.tsx --backup

# Dry-run режим
./target/release/inline-to-css-module ./src/components/MyComponent.tsx --dry-run

# Указание директории для CSS
./target/release/inline-to-css-module ./src/components/MyComponent.tsx -o ./src/styles
```

### 9. Пример использования

```bash
$ ./target/release/inline-to-css-module ./src/App.tsx

🚀 Inline Style to CSS Module Converter
📖 Processing: ./src/App.tsx

🔍 Found 3 styles with constant properties
   • Fully constant: 2
   • Mixed (with dynamic): 1

⚠️  Mixed styles (constants + dynamic):
   • <div>: constants: fontSize, padding | dynamic: color

✅ Created CSS file: ./src/App.module.css
   • Rules created: 3
💾 Backup saved: ./src/App.tsx.bak
✅ File updated: ./src/App.tsx

💡 Recommendation:
   Check files with mixed styles.
   Constant properties moved to CSS, dynamic remain in style.
```

## Преимущества Rust-версии

1. **🚀 Скорость** - в разы быстрее Python, особенно на больших проектах
2. **🛡️ Безопасность** - память управляется автоматически, нет ошибок времени выполнения
3. **📦 Единый бинарник** - не нужны внешние зависимости (Node.js, Python)
4. **🔧 Параллельная обработка** - легко добавить обработку нескольких файлов
5. **✅ Надежность** - статическая типизация гарантирует корректность
6. **🎯 Точное AST** - `oxc_parser` дает точное AST, лучше чем regex

Rust-версия идеально подходит для production использования! 🦀
