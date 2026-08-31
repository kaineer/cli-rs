use oxc::allocator::Allocator;
use oxc::ast::ast::*;
use oxc::ast::visit::{walk, Visit};
use oxc::parser::Parser;
use oxc::span::SourceType;
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
pub fn parse_jsx<'a>(source: &'a str, path: &Path) -> Result<Program<'a>> {
    let allocator = Box::new(Allocator::default());
    let allocator_ref: &'a Allocator = Box::leak(allocator);

    let source_type = SourceType::from_path(path).unwrap_or_else(|_| {
        if path.extension().map(|e| e == "tsx").unwrap_or(false) {
            SourceType::tsx()
        } else {
            SourceType::jsx()
        }
    });

    let parser = Parser::new(allocator_ref, source, source_type);
    let program = parser.parse();

    if !program.errors.is_empty() {
        let errors: Vec<String> = program
            .errors
            .iter()
            .map(|e| format!("{:?}", e))
            .collect();
        return Err(anyhow::anyhow!("Parse errors: {}", errors.join("\n")));
    }

    Ok(program.program)
}

/// Извлекает стили из AST
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

    /// Получает значение литерала
    fn get_literal_value(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::StringLiteral(lit) => Some(lit.value.to_string()),
            Expression::NumericLiteral(lit) => Some(lit.value.to_string()),
            Expression::BooleanLiteral(lit) => Some(lit.value.to_string()),
            Expression::NullLiteral(_) => Some("null".to_string()),
            _ => None,
        }
    }

    /// Извлекает имя ключа из PropertyKey
    fn get_key_name(&self, key: &PropertyKey) -> Option<String> {
        match key {
            PropertyKey::Identifier(ident) => Some(ident.name.to_string()),
            PropertyKey::StringLiteral(lit) => Some(lit.value.to_string()),
            PropertyKey::NumericLiteral(lit) => Some(lit.value.to_string()),
            PropertyKey::StaticIdentifier(ident) => Some(ident.name.to_string()),
            _ => None,
        }
    }

    /// Извлекает свойства из объекта
    fn extract_object_properties(
        &self,
        obj: &ObjectExpression,
    ) -> (HashMap<String, String>, HashMap<String, bool>) {
        let mut const_props = HashMap::new();
        let mut dynamic_props = HashMap::new();

        for prop in &obj.properties {
            if let ObjectPropertyKind::ObjectProperty(prop) = prop {
                if let Some(key) = self.get_key_name(&prop.key) {
                    let value_expr = &prop.value;

                    if let Some(lit_value) = self.get_literal_value(value_expr) {
                        let value = if key == "opacity" || key == "zIndex" || key == "flex" {
                            lit_value
                        } else if lit_value.parse::<f64>().is_ok() && !lit_value.contains('.') {
                            format!("{}px", lit_value)
                        } else {
                            lit_value
                        };
                        const_props.insert(key, value);
                    } else {
                        dynamic_props.insert(key, true);
                    }
                } else {
                    dynamic_props.insert("__computed_key__".to_string(), true);
                }
            }
        }

        (const_props, dynamic_props)
    }

    /// Находит полный диапазон атрибута, включая пробелы перед ним
    fn find_full_attribute_range(&self, attr_start: usize, attr_end: usize) -> (usize, usize) {
        let source = &self.source;
        
        // Ищем начало атрибута (идем назад до пробела или открывающей скобки)
        let mut start = attr_start;
        while start > 0 {
            let ch = source.chars().nth(start - 1).unwrap_or(' ');
            if ch == ' ' || ch == '\t' || ch == '\n' || ch == '<' || ch == '{' {
                break;
            }
            start -= 1;
        }
        
        // Ищем конец атрибута (идем вперед до пробела или закрывающей скобки)
        let mut end = attr_end;
        while end < source.len() {
            let ch = source.chars().nth(end).unwrap_or(' ');
            if ch == ' ' || ch == '\t' || ch == '\n' || ch == '>' || ch == '/' || ch == '}' {
                break;
            }
            end += 1;
        }
        
        (start, end)
    }

    /// Рекурсивно обходит AST и собирает стили
    pub fn extract(&mut self, program: &Program) {
        self.visit_program(program);
    }
}

impl<'a> Visit<'a> for StyleExtractor {
    fn visit_jsx_element(&mut self, element: &JSXElement<'a>) {
        let tag_name = match &element.opening_element.name {
            JSXElementName::Identifier(ident) => ident.name.to_string().to_lowercase(),
            _ => "div".to_string(),
        };

        for attr in &element.opening_element.attributes {
            match attr {
                JSXAttributeItem::Attribute(attr) => {
                    let attr_name = match &attr.name {
                        JSXAttributeName::Identifier(ident) => ident.name.to_string(),
                        _ => continue,
                    };

                    if attr_name == "style" {
                        if let Some(value) = &attr.value {
                            match value {
                                JSXAttributeValue::ExpressionContainer(container) => {
                                    if let Some(expr) = container.expression.as_expression() {
                                        if let Expression::ObjectExpression(obj_expr) = expr {
                                            let (const_props, dynamic_props) =
                                                self.extract_object_properties(obj_expr);

                                            if !const_props.is_empty() {
                                                let span = attr.span;
                                                // Находим полный диапазон атрибута
                                                let (start, end) = self.find_full_attribute_range(
                                                    span.start as usize,
                                                    span.end as usize,
                                                );
                                                
                                                self.mappings.push(StyleMapping {
                                                    tag_name: tag_name.clone(),
                                                    const_props,
                                                    dynamic_props,
                                                    start,
                                                    end,
                                                });
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        walk::walk_jsx_element(self, element);
    }
}

/// Основная функция для извлечения стилей
pub fn extract_styles(program: &Program, source: &str) -> Result<Vec<StyleMapping>> {
    let mut extractor = StyleExtractor::new(source);
    extractor.visit_program(program);
    Ok(extractor.mappings)
}
