use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

use crate::parser::StyleMapping;

/// Трансформирует JSX код, заменяя стили на className + style
pub fn transform_jsx(source: &str, mappings: &[StyleMapping]) -> Result<String> {
    let mut result = source.to_string();
    let mut tag_counter: HashMap<String, usize> = HashMap::new();

    // Обрабатываем в обратном порядке
    for mapping in mappings.iter().rev() {
        let rule_name = generate_rule_name(&mapping.tag_name, &mut tag_counter);

        // Находим точную позицию атрибута style
        // Используем диапазон из mapping
        let start = mapping.start;
        let end = mapping.end;

        // Проверяем, что индексы валидны
        if start >= result.len() || end > result.len() {
            continue;
        }

        // Получаем текст атрибута
        let attr_text = &result[start..end];
        
        // Проверяем, что это действительно style атрибут
        if !attr_text.contains("style") {
            continue;
        }

        // Создаем замену
        let replacement = if !mapping.dynamic_props.is_empty() {
            // Смешанный случай
            let new_style = remove_const_props(attr_text, &mapping.const_props)?;
            let clean_style = clean_style_object(&new_style);
            
            if clean_style.trim() == "style={}" || clean_style.trim() == "{}" {
                format!("className={{classes.{}}}", rule_name)
            } else {
                format!("{} className={{classes.{}}}", clean_style, rule_name)
            }
        } else {
            // Полностью константный
            format!("className={{classes.{}}}", rule_name)
        };

        // Заменяем
        result.replace_range(start..end, &replacement);
    }

    Ok(result)
}

/// Очищает объект стиля от лишних символов
fn clean_style_object(style: &str) -> String {
    let mut result = style.to_string();
    
    // Убираем лишние запятые и пробелы
    result = result.replace(", }", "}");
    result = result.replace("{ ,", "{");
    result = result.replace(", ,", ",");
    result = result.replace(",,", ",");
    result = result.replace("{ ", "{");
    result = result.replace(" }", "}");
    
    if result.trim() == "style={}" || result.trim() == "{}" {
        return "{}".to_string();
    }
    
    result
}

/// Удаляет константные свойства из объекта style
fn remove_const_props(style_content: &str, const_props: &HashMap<String, String>) -> Result<String> {
    let mut result = style_content.to_string();

    for prop in const_props.keys() {
        // Паттерны для удаления свойств
        let patterns = vec![
            format!(r#"{}\s*:\s*[^,}}]+[,}}]"#, prop),
            format!(r#"{}\s*:\s*['"][^'"]+['"][,}}]"#, prop),
        ];

        for pattern in patterns {
            let re = Regex::new(&pattern)?;
            result = re.replace_all(&result, "").to_string();
        }
    }

    // Чистим от лишних запятых
    let result = result
        .replace(", }", "}")
        .replace("{ ,", "{")
        .replace(", ,", ",")
        .replace(",,", ",")
        .replace("{}", "")
        .trim()
        .to_string();

    if result == "style={}" || result == "{}" || result.is_empty() {
        return Ok("{}".to_string());
    }

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
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("styles");

    let import_stmt = format!("import classes from \"./{}.module.css\";", stem);

    if source.contains("module.css") {
        return Ok(source.to_string());
    }

    let lines: Vec<&str> = source.lines().collect();
    let mut result = Vec::new();
    let mut inserted = false;
    let mut import_index = 0;

    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("import") && (line.contains("React") || line.contains("react")) {
            import_index = i + 1;
        } else if line.starts_with("import") && import_index > 0 {
            import_index = i + 1;
        } else if line.starts_with("import") && i < 10 {
            import_index = i + 1;
        }
    }

    for (i, line) in lines.iter().enumerate() {
        if i == import_index && !inserted {
            result.push(import_stmt.as_str());
            inserted = true;
        }
        result.push(line);
    }

    if !inserted {
        result.insert(0, import_stmt.as_str());
    }

    Ok(result.join("\n"))
}
