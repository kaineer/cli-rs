// Debug dump of a parsed schema.

use crate::parse::{Field, Schema, Skipped, ValueType, Widget};

pub fn dump(schema: &Schema) {
    println!("schema: {} field(s)", schema.fields.len());
    for field in &schema.fields {
        dump_field(field);
    }
    if !schema.skipped.is_empty() {
        println!();
        println!("skipped: {} key(s)", schema.skipped.len());
        for s in &schema.skipped {
            dump_skipped(s);
        }
    }
}

fn dump_field(field: &Field) {
    println!();
    println!("  [{}]", field.key);
    println!("    value_type: {}", fmt_value_type(&field.value_type));
    println!("    widget:     {}", fmt_widget(&field.widget));
    match &field.default {
        Some(v) => println!("    default:    {v:?}"),
        None => println!("    default:    <undefined>"),
    }
    match &field.label {
        Some(v) => println!("    label:      {v}"),
        None => println!("    label:      <none>"),
    }
}

fn dump_skipped(s: &Skipped) {
    println!("  [{}] — {}", s.key, s.reason);
}

fn fmt_value_type(t: &ValueType) -> String {
    match t {
        ValueType::String => "string".into(),
        ValueType::Boolean => "boolean".into(),
        ValueType::Number => "number".into(),
        ValueType::Other(s) => format!("other({s})"),
    }
}

fn fmt_widget(w: &Widget) -> String {
    match w {
        Widget::Text => "text".into(),
        Widget::Password => "password".into(),
        Widget::Textarea { height } => match height {
            Some(h) => format!("textarea(height={h})"),
            None => "textarea".into(),
        },
        Widget::Select { options } => {
            let opts: Vec<String> = options
                .iter()
                .map(|o| {
                    if o.label == o.value {
                        o.label.clone()
                    } else {
                        format!("{}({})", o.label, o.value)
                    }
                })
                .collect();
            format!("select[{}]", opts.join(" | "))
        }
        Widget::Checkbox => "checkbox".into(),
        Widget::Enable { on, off } => {
            let on = on.as_deref().unwrap_or("enabled");
            let off = off.as_deref().unwrap_or("disabled");
            format!("enable(on={on}, off={off})")
        }
        Widget::Other(s) => format!("other({s})"),
    }
}
