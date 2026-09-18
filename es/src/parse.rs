// Schema types and compact-DSL parser for es.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_yaml::Value;

/// Тип значения в выходном YAML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueType {
    String,
    Boolean,
    Number,
    /// Неизвестный / пока не поддерживаемый тип из схемы.
    Other(String),
}

/// Виджет TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Widget {
    Text,
    Password,
    Textarea { height: Option<u32> },
    Select { options: Vec<SelectOption> },
    Checkbox,
    /// Bool с подписями enabled/disabled (кастомизируемыми).
    Enable { on: Option<String>, off: Option<String> },
    /// Неизвестный виджет.
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
}

/// Лист схемы (плоское поле).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub value_type: ValueType,
    pub widget: Widget,
    /// Строковое default из компактного DSL; `None` = undefined.
    pub default: Option<String>,
    pub label: Option<String>,
}

/// Разобранная схема (пока только плоский список полей).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    pub fields: Vec<Field>,
    /// Ключи, пропущенные из‑за вложенности или неподдерживаемой формы.
    pub skipped: Vec<Skipped>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub key: String,
    pub reason: String,
}

pub fn parse_file(path: &Path) -> Result<Schema> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("не удалось прочитать {}", path.display()))?;
    parse_str(&text).with_context(|| format!("ошибка разбора схемы {}", path.display()))
}

pub fn parse_str(text: &str) -> Result<Schema> {
    let root: Value = serde_yaml::from_str(text).context("невалидный YAML")?;
    let Value::Mapping(map) = root else {
        bail!("корень схемы должен быть YAML-мапой");
    };

    let mut fields = Vec::new();
    let mut skipped = Vec::new();

    for (key, value) in map {
        let key = match key.as_str() {
            Some(s) => s.to_string(),
            None => {
                skipped.push(Skipped {
                    key: format!("{key:?}"),
                    reason: "ключ не строка".into(),
                });
                continue;
            }
        };

        match value {
            Value::String(s) => match parse_leaf(&key, &s) {
                Ok(field) => fields.push(field),
                Err(e) => skipped.push(Skipped {
                    key,
                    reason: e.to_string(),
                }),
            },
            Value::Mapping(_) => skipped.push(Skipped {
                key,
                reason: "вложенные объекты пока не поддерживаются".into(),
            }),
            other => skipped.push(Skipped {
                key,
                reason: format!("ожидалась строка компактного DSL, получено {other:?}"),
            }),
        }
    }

    Ok(Schema { fields, skipped })
}

fn parse_leaf(key: &str, dsl: &str) -> Result<Field> {
    let dsl = dsl.trim();
    if dsl.is_empty() {
        bail!("пустое описание поля");
    }

    // `boolean` ≡ `boolean:widget=checkbox`
    if dsl == "boolean" {
        return Ok(Field {
            key: key.to_string(),
            value_type: ValueType::Boolean,
            widget: Widget::Checkbox,
            default: None,
            label: None,
        });
    }

    let (type_part, params_part) = match dsl.split_once(':') {
        Some((t, p)) => (t.trim(), p.trim()),
        None => bail!("ожидалось `value_type:params` или `boolean`, получено `{dsl}`"),
    };

    if type_part.is_empty() || params_part.is_empty() {
        bail!("неполное описание поля `{dsl}`");
    }

    let value_type = parse_value_type(type_part);
    let params = parse_params(params_part)?;

    let widget_name = params
        .iter()
        .find(|(k, _)| k == "widget")
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("нет обязательного параметра widget"))?;

    let height = params
        .iter()
        .find(|(k, _)| k == "height")
        .map(|(_, v)| v.parse::<u32>())
        .transpose()
        .context("height должен быть числом")?;

    let widget = parse_widget(widget_name, height, &params)?;

    let default = params
        .iter()
        .find(|(k, _)| k == "default")
        .map(|(_, v)| v.clone());
    let label = params
        .iter()
        .find(|(k, _)| k == "label")
        .map(|(_, v)| v.clone());

    Ok(Field {
        key: key.to_string(),
        value_type,
        widget,
        default,
        label,
    })
}

fn parse_value_type(s: &str) -> ValueType {
    match s {
        "string" => ValueType::String,
        "boolean" => ValueType::Boolean,
        "number" | "integer" => ValueType::Number,
        other => ValueType::Other(other.to_string()),
    }
}

fn parse_params(s: &str) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((k, v)) = part.split_once('=') else {
            bail!("параметр без `=`: `{part}`");
        };
        let k = k.trim();
        let v = v.trim();
        if k.is_empty() {
            bail!("пустое имя параметра в `{part}`");
        }
        out.push((k.to_string(), v.to_string()));
    }
    Ok(out)
}

fn parse_widget(
    name: &str,
    height: Option<u32>,
    params: &[(String, String)],
) -> Result<Widget> {
    match name {
        "text" => Ok(Widget::Text),
        "password" => Ok(Widget::Password),
        "textarea" => Ok(Widget::Textarea { height }),
        "checkbox" => Ok(Widget::Checkbox),
        "enable" => {
            let on = params
                .iter()
                .find(|(k, _)| k == "on")
                .map(|(_, v)| v.clone());
            let off = params
                .iter()
                .find(|(k, _)| k == "off")
                .map(|(_, v)| v.clone());
            Ok(Widget::Enable { on, off })
        }
        "select" => {
            let options_raw = params
                .iter()
                .find(|(k, _)| k == "options")
                .map(|(_, v)| v.as_str())
                .unwrap_or("");
            Ok(Widget::Select {
                options: parse_options(options_raw),
            })
        }
        other => Ok(Widget::Other(other.to_string())),
    }
}

fn parse_options(raw: &str) -> Vec<SelectOption> {
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split('|')
        .map(|opt| {
            let opt = opt.trim();
            if let Some((label, rest)) = opt.split_once('(') {
                let value = rest.trim_end_matches(')').to_string();
                SelectOption {
                    label: label.to_string(),
                    value,
                }
            } else {
                SelectOption {
                    label: opt.to_string(),
                    value: opt.to_string(),
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_text_and_boolean() {
        let schema = parse_str(
            r#"
name: string:widget=text,default=John,label=Имя
present: boolean
"#,
        )
        .unwrap();
        assert_eq!(schema.fields.len(), 2);
        assert_eq!(schema.fields[0].key, "name");
        assert_eq!(schema.fields[0].value_type, ValueType::String);
        assert_eq!(schema.fields[0].widget, Widget::Text);
        assert_eq!(schema.fields[0].default.as_deref(), Some("John"));
        assert_eq!(schema.fields[0].label.as_deref(), Some("Имя"));
        assert_eq!(schema.fields[1].widget, Widget::Checkbox);
        assert_eq!(schema.fields[1].value_type, ValueType::Boolean);
    }

    #[test]
    fn parse_enable() {
        let schema = parse_str(
            r#"
feature: boolean:widget=enable
feature_ru: boolean:widget=enable,on=вкл,off=выкл
"#,
        )
        .unwrap();
        assert_eq!(
            schema.fields[0].widget,
            Widget::Enable {
                on: None,
                off: None
            }
        );
        assert_eq!(
            schema.fields[1].widget,
            Widget::Enable {
                on: Some("вкл".into()),
                off: Some("выкл".into())
            }
        );
    }

    #[test]
    fn skip_nested() {
        let schema = parse_str(
            r#"
present: boolean
sides:
  left: boolean
"#,
        )
        .unwrap();
        assert_eq!(schema.fields.len(), 1);
        assert_eq!(schema.skipped.len(), 1);
        assert_eq!(schema.skipped[0].key, "sides");
    }
}
