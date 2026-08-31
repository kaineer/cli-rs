use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;

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
    })
    .to_string()
    .to_lowercase()
}
