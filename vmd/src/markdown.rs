//! Разбор markdown в плоский список блоков для отрисовки.
//!
//! Подсветки кода нет: блок кода — это моноширинный текст на своём фоне.
//! Заголовки, списки, цитаты, таблицы и разделители дают разный вид и отступы.
//!
//! Внутри текста сохраняется строчная разметка (`**жирный**`, `*курсив*`,
//! `` `код` ``, `~~зачёркнутый~~`) — она приходит как список спанов.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Что за блок перед нами — определяет стиль при отрисовке.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    Paragraph,
    Code,
    Quote,
    ListItem,
    Rule,
    TableRow,
}

/// Строчный стиль фрагмента текста.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
}

/// Кусок текста с одинаковым строчным стилем.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: InlineStyle,
}

impl Span {
    fn new(text: impl Into<String>, style: InlineStyle) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    pub spans: Vec<Span>,
    /// Уровень вложенности списка (0 — верхний).
    pub indent: usize,
    /// Для элементов списка: маркер.
    pub marker: Option<String>,
}

impl Block {
    /// Сплошной текст без разметки — для тестов и поиска.
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

/// Состояние одного открытого элемента списка.
struct Item {
    indent: usize,
    marker: String,
}

pub fn parse(markdown: &str) -> Vec<Block> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut state = State::default();

    for event in Parser::new_ext(markdown, options) {
        state.handle(event);
    }

    state.blocks
}

#[derive(Default)]
struct State {
    blocks: Vec<Block>,
    spans: Vec<Span>,
    /// Накопленный текст до сброса в спаны.
    buf: String,
    kind: Option<BlockKind>,
    /// Счётчики открытых нумерованных списков (`None` — маркированный).
    lists: Vec<Option<u64>>,
    items: Vec<Item>,
    in_code: bool,
    /// Глубина вложенности цитат.
    quote_depth: usize,
    /// Текущий строчный стиль.
    style: InlineStyle,
}

impl State {
    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) | Event::Html(t) => self.push(&t),
            Event::Code(t) => {
                let previous = self.style;
                self.flush_spans();
                self.style.code = true;
                self.push(&t);
                self.flush_spans();
                self.style = previous;
            }
            Event::SoftBreak => {
                self.push(if self.in_code { "\n" } else { " " });
            }
            Event::HardBreak => self.push("\n"),
            Event::Rule => self.blocks.push(Block {
                kind: BlockKind::Rule,
                spans: Vec::new(),
                indent: 0,
                marker: None,
            }),
            Event::TaskListMarker(done) => {
                self.push(if done { "[x] " } else { "[ ] " });
            }
            _ => {}
        }
    }

    fn push(&mut self, text: &str) {
        if !text.is_empty() {
            self.buf.push_str(text);
        }
    }

    /// Переносит буфер в список спанов текущим стилем.
    fn flush_spans(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.buf);
        // Соседние куски с тем же стилем склеиваем.
        if let Some(last) = self.spans.last_mut() {
            if last.style == self.style {
                last.text.push_str(&text);
                return;
            }
        }
        self.spans.push(Span::new(text, self.style));
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush_pending();
                self.kind = Some(match level as u8 {
                    1 => BlockKind::H1,
                    2 => BlockKind::H2,
                    3 => BlockKind::H3,
                    4 => BlockKind::H4,
                    5 => BlockKind::H5,
                    _ => BlockKind::H6,
                });
            }
            Tag::Paragraph => {
                if self.items.is_empty() {
                    self.flush_pending();
                    self.kind = Some(if self.quote_depth > 0 {
                        BlockKind::Quote
                    } else {
                        BlockKind::Paragraph
                    });
                } else {
                    // Второй абзац внутри пункта — с пустой строкой.
                    self.flush_spans();
                    if !self.spans.is_empty() {
                        self.push("\n");
                        self.flush_spans();
                    }
                }
            }
            Tag::CodeBlock(_) => {
                self.flush_pending();
                self.in_code = true;
                self.kind = Some(BlockKind::Code);
            }
            Tag::BlockQuote(_) => {
                self.flush_pending();
                self.quote_depth += 1;
                self.kind = Some(BlockKind::Quote);
            }
            Tag::List(start) => self.lists.push(start),
            Tag::Item => {
                // Вложенный список приходит до `End(Item)`, так что текст
                // родительского пункта надо сохранить прямо сейчас.
                self.flush_item();
                let indent = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let label = format!("{n}.");
                        *n += 1;
                        label
                    }
                    _ => "•".to_string(),
                };
                self.items.push(Item { indent, marker });
            }
            Tag::Strong => {
                self.flush_spans();
                self.style.bold = true;
            }
            Tag::Emphasis => {
                self.flush_spans();
                self.style.italic = true;
            }
            Tag::Strikethrough => {
                self.flush_spans();
                self.style.strike = true;
            }
            Tag::TableHead | Tag::TableRow => {
                self.flush_pending();
                self.spans.clear();
            }
            Tag::TableCell => {
                // Ячейки разделяем двумя пробелами — в моноширинном выводе
                // этого достаточно.
                self.flush_spans();
                if !self.spans.is_empty() {
                    self.push("  ");
                    self.flush_spans();
                }
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => self.flush_kind(),
            TagEnd::Paragraph => {
                if self.items.is_empty() {
                    self.flush_kind();
                }
            }
            TagEnd::CodeBlock => {
                self.in_code = false;
                self.flush_kind();
            }
            TagEnd::BlockQuote(_) => {
                self.flush_kind();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            TagEnd::Item => {
                self.flush_item();
                self.items.pop();
            }
            TagEnd::List(_) => {
                self.lists.pop();
            }
            TagEnd::Strong => {
                self.flush_spans();
                self.style.bold = false;
            }
            TagEnd::Emphasis => {
                self.flush_spans();
                self.style.italic = false;
            }
            TagEnd::Strikethrough => {
                self.flush_spans();
                self.style.strike = false;
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                self.flush_spans();
                let spans = trim_spans(std::mem::take(&mut self.spans));
                if !spans.is_empty() {
                    self.blocks.push(Block {
                        kind: BlockKind::TableRow,
                        spans,
                        indent: 0,
                        marker: None,
                    });
                }
            }
            _ => {}
        }
    }

    /// Сохраняет накопленное как блок текущего вида.
    fn flush_kind(&mut self) {
        let Some(kind) = self.kind.take() else {
            return;
        };
        self.flush_spans();
        let spans = std::mem::take(&mut self.spans);
        let spans = if kind == BlockKind::Code {
            trim_code_spans(spans)
        } else {
            trim_spans(spans)
        };
        if spans.is_empty() && kind != BlockKind::Code {
            return;
        }
        self.blocks.push(Block {
            kind,
            spans,
            indent: 0,
            marker: None,
        });
    }

    /// Сохраняет накопленное как элемент списка.
    fn flush_item(&mut self) {
        let Some((indent, marker)) = self
            .items
            .last()
            .map(|item| (item.indent, item.marker.clone()))
        else {
            return;
        };
        self.flush_spans();
        let spans = trim_spans(std::mem::take(&mut self.spans));
        self.kind = None;
        if spans.is_empty() {
            return;
        }
        self.blocks.push(Block {
            kind: BlockKind::ListItem,
            spans,
            indent,
            marker: Some(marker),
        });
    }

    /// Незавершённый абзац/заголовок перед новым блоком.
    fn flush_pending(&mut self) {
        if self.kind.is_some() {
            self.flush_kind();
            return;
        }
        if !self.items.is_empty() {
            self.flush_item();
        }
    }
}

fn trim_spans(spans: Vec<Span>) -> Vec<Span> {
    trim(spans, true, true)
}

/// Блок кода: слева пробелы значимы, справа — только хвостовые переводы.
fn trim_code_spans(spans: Vec<Span>) -> Vec<Span> {
    trim(spans, false, true)
}

fn trim(spans: Vec<Span>, start: bool, end: bool) -> Vec<Span> {
    let mut spans: Vec<Span> = spans.into_iter().filter(|s| !s.text.is_empty()).collect();

    if start {
        while let Some(first) = spans.first_mut() {
            let rest = first.text.trim_start().to_string();
            let changed = rest.len() != first.text.len();
            first.text = rest;
            if first.text.is_empty() {
                spans.remove(0);
            } else if !changed {
                break;
            }
        }
    }

    if end {
        while let Some(last) = spans.last_mut() {
            let rest = last.text.trim_end_matches([' ', '\n', '\t', '\r']).to_string();
            let changed = rest.len() != last.text.len();
            last.text = rest;
            if last.text.is_empty() {
                spans.pop();
            } else if !changed {
                break;
            }
        }
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_get_levels() {
        let blocks = parse("# one\n\n### three\n");
        assert_eq!(blocks[0].kind, BlockKind::H1);
        assert_eq!(blocks[0].text(), "one");
        assert_eq!(blocks[1].kind, BlockKind::H3);
        assert_eq!(blocks[1].text(), "three");
    }

    #[test]
    fn code_block_keeps_lines() {
        let blocks = parse("```\nlet a = 1;\nlet b = 2;\n```\n");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, BlockKind::Code);
        assert_eq!(blocks[0].text(), "let a = 1;\nlet b = 2;");
    }

    #[test]
    fn unordered_and_ordered_lists() {
        let blocks = parse("- a\n- b\n\n1. one\n2. two\n");
        assert_eq!(blocks.len(), 4);
        assert!(blocks.iter().all(|b| b.kind == BlockKind::ListItem));
        assert_eq!(blocks[0].marker.as_deref(), Some("•"));
        assert_eq!(blocks[0].text(), "a");
        assert_eq!(blocks[2].marker.as_deref(), Some("1."));
        assert_eq!(blocks[3].marker.as_deref(), Some("2."));
    }

    #[test]
    fn nested_list_has_indent() {
        let blocks = parse("- a\n  - b\n");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].text(), "a");
        assert_eq!(blocks[0].indent, 0);
        assert_eq!(blocks[1].text(), "b");
        assert_eq!(blocks[1].indent, 1);
    }

    #[test]
    fn quote_and_rule() {
        let blocks = parse("> quoted\n\n---\n");
        assert_eq!(blocks[0].kind, BlockKind::Quote);
        assert_eq!(blocks[0].text(), "quoted");
        assert_eq!(blocks[1].kind, BlockKind::Rule);
    }

    #[test]
    fn paragraph_wraps_soft_break() {
        let blocks = parse("one\ntwo\n");
        assert_eq!(blocks[0].text(), "one two");
    }

    #[test]
    fn inline_code_joins_text() {
        let blocks = parse("use `foo` here\n");
        assert_eq!(blocks[0].text(), "use foo here");
    }

    #[test]
    fn text_around_heading_keeps_order() {
        let blocks = parse("intro\n\n## mid\n\nafter\n");
        assert_eq!(
            blocks.iter().map(|b| b.kind).collect::<Vec<_>>(),
            vec![BlockKind::Paragraph, BlockKind::H2, BlockKind::Paragraph]
        );
    }

    #[test]
    fn bold_marks_span() {
        let blocks = parse("a **bold** b\n");
        assert_eq!(blocks[0].text(), "a bold b");
        let bold: Vec<_> = blocks[0].spans.iter().filter(|s| s.style.bold).collect();
        assert_eq!(bold.len(), 1);
        assert_eq!(bold[0].text, "bold");
        assert!(!bold[0].style.italic);
    }

    #[test]
    fn italic_marks_span() {
        let blocks = parse("a *it* b\n");
        assert_eq!(blocks[0].text(), "a it b");
        let italic: Vec<_> = blocks[0].spans.iter().filter(|s| s.style.italic).collect();
        assert_eq!(italic.len(), 1);
        assert_eq!(italic[0].text, "it");
    }

    #[test]
    fn bold_italic_combined() {
        let blocks = parse("**bold *both* bold**\n");
        let both: Vec<_> = blocks[0]
            .spans
            .iter()
            .filter(|s| s.style.bold && s.style.italic)
            .collect();
        assert_eq!(both.len(), 1);
        assert_eq!(both[0].text, "both");
        let outer: Vec<_> = blocks[0]
            .spans
            .iter()
            .filter(|s| s.style.bold && !s.style.italic)
            .collect();
        assert_eq!(outer.len(), 2);
    }

    #[test]
    fn inline_code_has_own_style() {
        let blocks = parse("run `cargo test` now\n");
        let code: Vec<_> = blocks[0].spans.iter().filter(|s| s.style.code).collect();
        assert_eq!(code.len(), 1);
        assert_eq!(code[0].text, "cargo test");
        assert!(!code[0].style.bold);
    }

    #[test]
    fn code_inside_bold_keeps_both() {
        let blocks = parse("**важно: `x = 1`**\n");
        let span = blocks[0]
            .spans
            .iter()
            .find(|s| s.style.code)
            .expect("нет спана кода");
        assert!(span.style.bold);
        assert_eq!(span.text, "x = 1");
    }

    #[test]
    fn strikethrough_marks_span() {
        let blocks = parse("старое ~~нет~~ новое\n");
        let strike: Vec<_> = blocks[0].spans.iter().filter(|s| s.style.strike).collect();
        assert_eq!(strike.len(), 1);
        assert_eq!(strike[0].text, "нет");
    }

    #[test]
    fn adjacent_same_style_is_merged() {
        // Две подряд идущие курсивные части должны склеиться в один спан.
        let blocks = parse("*a*_b_\n");
        assert_eq!(blocks[0].text(), "ab");
        assert_eq!(blocks[0].spans.len(), 1);
        assert!(blocks[0].spans[0].style.italic);
    }

    #[test]
    fn heading_keeps_inline_styles() {
        let blocks = parse("## *важно* и **сильно**\n");
        assert_eq!(blocks[0].kind, BlockKind::H2);
        assert_eq!(blocks[0].text(), "важно и сильно");
        assert!(blocks[0].spans.iter().any(|s| s.style.italic));
        assert!(blocks[0].spans.iter().any(|s| s.style.bold));
    }

    #[test]
    fn list_item_keeps_inline_styles() {
        let blocks = parse("- **жирный** пункт\n");
        assert_eq!(blocks[0].kind, BlockKind::ListItem);
        assert_eq!(blocks[0].text(), "жирный пункт");
        assert!(blocks[0].spans.iter().any(|s| s.style.bold));
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        let blocks = parse("  hello  \n");
        assert_eq!(blocks[0].text(), "hello");
    }
}
