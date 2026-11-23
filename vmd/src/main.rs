//! vmd — просмотр markdown в окне gpui.
//!
//! Заголовки, списки и блоки кода отображаются всегда; подсветки кода нет.
//! Прокрутка — vim-действиями: `j`/`k`, `gg`, `G`, плюс `d`/`u` и колесо мыши.

mod markdown;

use std::path::PathBuf;
use std::process;

use gpui::{
    actions, div, font, point, prelude::*, px, rgb, AnyElement, App, Application, Bounds, Context,
    Div, FocusHandle, Font, FontStyle, FontWeight, ScrollHandle, SharedString, StrikethroughStyle,
    StyledText, TextRun, Window, WindowBounds, WindowOptions,
};

use markdown::{Block, BlockKind, InlineStyle};

actions!(vmd, [LineDown, LineUp, PageDown, PageUp, ToTop, ToBottom]);

// ---- Nord-палитра ----

fn fg_bg() -> gpui::Rgba {
    rgb(0x2e3440)
}
fn fg_text() -> gpui::Rgba {
    rgb(0xd8dee9)
}
fn fg_muted() -> gpui::Rgba {
    rgb(0x616e88)
}
fn fg_heading() -> gpui::Rgba {
    rgb(0x88c0d0)
}
fn fg_accent() -> gpui::Rgba {
    rgb(0x8fbcbb)
}
fn fg_code_bg() -> gpui::Rgba {
    rgb(0x3b4252)
}
fn fg_quote() -> gpui::Rgba {
    rgb(0xb48ead)
}
fn fg_rule() -> gpui::Rgba {
    rgb(0x4c566a)
}

// ---- Параметры вида ----

const BASE_FONT: f32 = 15.0;
/// Сколько пикселей прокручивать на одну строку.
const LINE_STEP: f32 = 52.0;
/// Отступ на один уровень вложенности списка.
const INDENT_STEP: f32 = 24.0;
const PAGE_MARGIN: f32 = 48.0;
const PROSE_WIDTH: f32 = 760.0;

fn mono_family() -> SharedString {
    SharedString::from("FiraCode Nerd Font Mono")
}

/// Семейства, у которых в системе есть Regular/Bold/Italic/BoldItalic.
/// `.SystemUIFont` на Linux жёстко указывает на IBM Plex Sans, которого
/// здесь нет, и gpui вынужден уходить в fallback теряя вес и наклон.
const UI_FAMILIES: [&str; 4] = ["Noto Sans", "Inter", "DejaVu Sans", "Liberation Sans"];

/// Первое семейство из списка, которое реально установлено.
fn ui_family(cx: &App) -> SharedString {
    let available = cx.text_system().all_font_names();
    UI_FAMILIES
        .iter()
        .find(|family| available.iter().any(|name| name == *family))
        .map(|family| SharedString::from(*family))
        .unwrap_or_else(|| SharedString::from(".SystemUIFont"))
}

/// Шрифт с наклоном/насыщенностью. `base` — семейство основного текста
/// блока: для параграфов UI-шрифт, для блоков кода — моноширинный.
/// Inline-код всегда моноширинный, даже внутри обычного абзаца.
fn span_font(style: InlineStyle, weight: FontWeight, base: &SharedString) -> Font {
    let family = if style.code { mono_family() } else { base.clone() };
    Font {
        weight: if style.bold { FontWeight::BOLD } else { weight },
        style: if style.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        },
        ..font(family.clone())
    }
}

/// Собирает `StyledText` из спанов: жирный, курсив, inline-код, зачёркивание.
fn styled_spans(
    block: &Block,
    weight: FontWeight,
    color: gpui::Rgba,
    code_background: Option<gpui::Rgba>,
    base_family: &SharedString,
) -> StyledText {
    let text: String = block.spans.iter().map(|s| s.text.as_str()).collect();
    let mut runs: Vec<TextRun> = Vec::with_capacity(block.spans.len());

    for span in &block.spans {
        let font = span_font(span.style, weight, base_family);
        let mut run = TextRun {
            len: span.text.len(),
            font,
            color: color.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        if span.style.code {
            run.background_color = code_background.map(Into::into);
        }
        if span.style.strike {
            run.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.),
                color: Some(color.into()),
            });
        }
        runs.push(run);
    }

    StyledText::new(SharedString::from(text)).with_runs(runs)
}

struct Viewer {
    focus: FocusHandle,
    scroll: ScrollHandle,
    blocks: Vec<Block>,
    /// Основное семейство текста (без inline-кода).
    ui_family: SharedString,
    /// Высота видимой области, обновляется в render.
    viewport_height: f32,
}

impl Viewer {
    fn new(markdown: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ui_family = ui_family(cx);
        let focus = cx.focus_handle();
        focus.focus(window);
        Self {
            focus,
            scroll: ScrollHandle::new(),
            blocks: markdown::parse(markdown),
            ui_family,
            viewport_height: 760.0,
        }
    }

    // ---- прокрутка ----

    fn offset(&self) -> f32 {
        self.scroll.offset().y.into()
    }

    fn set_offset(&self, y: f32) {
        self.scroll.set_offset(point(px(0.), px(y)));
    }

    /// Положительное `dy` — вниз по документу, отрицательное — вверх.
    fn scroll_by(&mut self, dy: f32, cx: &mut Context<Self>) {
        let max: f32 = self.scroll.max_offset().height.into();
        let target = (self.offset() - dy).clamp(-max, 0.0);
        self.set_offset(target);
        cx.notify();
    }

    fn scroll_page(&mut self, down: bool, cx: &mut Context<Self>) {
        let page = (self.viewport_height - 2.0 * PAGE_MARGIN).max(LINE_STEP);
        self.scroll_by(if down { page } else { -page }, cx);
    }

    fn scroll_to_top(&mut self, cx: &mut Context<Self>) {
        self.set_offset(0.0);
        cx.notify();
    }

    fn scroll_to_bottom(&mut self, cx: &mut Context<Self>) {
        let max: f32 = self.scroll.max_offset().height.into();
        self.set_offset(-max);
        cx.notify();
    }

    // ---- блоки ----

    fn render_blocks(&self) -> Div {
        let mut column = div().flex().flex_col().w_full();
        for block in &self.blocks {
            column = column.child(self.render_block(block));
        }
        column
    }

    fn render_block(&self, block: &Block) -> AnyElement {
        let indent = px(block.indent as f32 * INDENT_STEP);

        match block.kind {
            BlockKind::H1 => self.heading_block(block, 32.0, 18.0),
            BlockKind::H2 => self.heading_block(block, 26.0, 15.0),
            BlockKind::H3 => self.heading_block(block, 21.0, 12.0),
            BlockKind::H4 => self.heading_block(block, 18.5, 10.0),
            BlockKind::H5 => self.heading_block(block, 16.5, 8.0),
            BlockKind::H6 => {
                let styled = styled_spans(block, FontWeight::BOLD, fg_muted(), Some(fg_code_bg()), &self.ui_family);
                div()
                    .mt(px(8.))
                    .mb_2()
                    .w_full()
                    .text_size(px(15.0))
                    .line_height(px(22.0))
                    .text_color(fg_muted())
                    .font_weight(FontWeight::BOLD)
                    .child(styled)
                    .into_any_element()
            }

            BlockKind::Paragraph => self.paragraph_block(block, indent),

            BlockKind::Code => {
                // Тот же кегль и цвет, что у inline-кода: `TextRun` не умеет
                // менять размер, поэтому блок и `code` иначе не совпадут.
                let styled =
                    styled_spans(block, FontWeight::NORMAL, fg_text(), None, &mono_family());
                div()
                    .mt_3()
                    .mb_3()
                    .w_full()
                    .bg(fg_code_bg())
                    .rounded_md()
                    .p_4()
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(px(BASE_FONT))
                            .line_height(px(BASE_FONT * 1.5))
                            .text_color(fg_text())
                            .child(styled),
                    )
                    .into_any_element()
            }

            BlockKind::Quote => div()
                .mt_2()
                .mb_2()
                .pl_4()
                .w_full()
                .border_l_3()
                .border_color(fg_quote())
                .child(
                    div()
                        .text_size(px(BASE_FONT))
                        .line_height(px(24.0))
                        .text_color(fg_muted())
                        .child(styled_spans(
                            block,
                            FontWeight::NORMAL,
                            fg_muted(),
                            Some(fg_code_bg()),
                            &self.ui_family,
                        )),
                )
                .into_any_element(),

            BlockKind::ListItem => div()
                .flex()
                .flex_row()
                .gap_2()
                .mt_1()
                .mb_1()
                .ml(indent)
                .w_full()
                .child(
                    div()
                        .w(px(22.))
                        .flex_shrink_0()
                        .text_size(px(BASE_FONT))
                        .text_color(fg_accent())
                        .child(SharedString::from(
                            block.marker.clone().unwrap_or_else(|| "•".into()),
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(px(BASE_FONT))
                        .line_height(px(24.0))
                        .text_color(fg_text())
                        .child(styled_spans(
                            block,
                            FontWeight::NORMAL,
                            fg_text(),
                            Some(fg_code_bg()),
                            &self.ui_family,
                        )),
                )
                .into_any_element(),

            BlockKind::TableRow => div()
                .w_full()
                .py_1()
                .border_b_1()
                .border_color(fg_rule())
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(px(13.5))
                        .text_color(fg_text())
                        .child(styled_spans(
                            block,
                            FontWeight::NORMAL,
                            fg_text(),
                            None,
                            &mono_family(),
                        )),
                )
                .into_any_element(),

            BlockKind::Rule => div()
                .my_4()
                .w_full()
                .h(px(1.))
                .bg(fg_rule())
                .into_any_element(),
        }
    }

    fn heading_block(&self, block: &Block, size: f32, margin_top: f32) -> AnyElement {
        let styled = styled_spans(block, FontWeight::BOLD, fg_heading(), Some(fg_code_bg()), &self.ui_family);
        div()
            .mt(px(margin_top))
            .mb_2()
            .w_full()
            .text_size(px(size))
            .line_height(px(size * 1.3))
            .text_color(fg_heading())
            .font_weight(FontWeight::BOLD)
            .child(styled)
            .into_any_element()
    }

    fn paragraph_block(&self, block: &Block, indent: gpui::Pixels) -> AnyElement {
        let styled = styled_spans(block, FontWeight::NORMAL, fg_text(), Some(fg_code_bg()), &self.ui_family);
        div()
            .mt_2()
            .mb_2()
            .ml(indent)
            .w_full()
            .text_size(px(BASE_FONT))
            .line_height(px(25.0))
            .text_color(fg_text())
            .child(styled)
            .into_any_element()
    }
}

impl Render for Viewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.viewport_height = window.viewport_size().height.into();

        div()
            .id("vmd-root")
            .size_full()
            .bg(fg_bg())
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &LineDown, _, cx| this.scroll_by(LINE_STEP, cx)))
            .on_action(cx.listener(|this, _: &LineUp, _, cx| this.scroll_by(-LINE_STEP, cx)))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| this.scroll_page(true, cx)))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| this.scroll_page(false, cx)))
            .on_action(cx.listener(|this, _: &ToTop, _, cx| this.scroll_to_top(cx)))
            .on_action(cx.listener(|this, _: &ToBottom, _, cx| this.scroll_to_bottom(cx)))
            .child(
                div()
                    .id("vmd-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .w_full()
                            .py(px(PAGE_MARGIN))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .w(px(PROSE_WIDTH))
                                    .child(self.render_blocks()),
                            ),
                    ),
            )
    }
}

fn bind_keys(cx: &mut App) {
    use gpui::KeyBinding;

    cx.bind_keys([
        KeyBinding::new("j", LineDown, None),
        KeyBinding::new("down", LineDown, None),
        KeyBinding::new("k", LineUp, None),
        KeyBinding::new("up", LineUp, None),
        KeyBinding::new("ctrl-d", PageDown, None),
        KeyBinding::new("space", PageDown, None),
        KeyBinding::new("ctrl-u", PageUp, None),
        KeyBinding::new("g g", ToTop, None),
        KeyBinding::new("shift-g", ToBottom, None),
    ]);
}

struct Args {
    path: PathBuf,
    fullscreen: bool,
}

fn usage() -> String {
    format!(
        "использование: {} [--fullscreen] <файл.md>",
        env!("CARGO_BIN_NAME")
    )
}

fn parse_args<I: Iterator<Item = String>>(mut args: I) -> Result<Args, String> {
    let mut path: Option<PathBuf> = None;
    let mut fullscreen = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fullscreen" | "-f" => fullscreen = true,
            "--help" | "-h" => return Err(usage()),
            other if other.starts_with('-') => {
                return Err(format!("неизвестная опция: {other}\n\n{}", usage()));
            }
            other => {
                if path.is_some() {
                    return Err(format!("лишний аргумент: {other}\n\n{}", usage()));
                }
                path = Some(PathBuf::from(other));
            }
        }
    }

    let Some(path) = path else {
        return Err(format!("нужно имя файла\n\n{}", usage()));
    };

    Ok(Args { path, fullscreen })
}

fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            process::exit(2);
        }
    };

    let markdown = match std::fs::read_to_string(&args.path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!("не удалось прочитать {}: {err}", args.path.display());
            process::exit(1);
        }
    };

    let path = args.path.clone();
    let fullscreen = args.fullscreen;

    Application::new().run(move |cx: &mut App| {
        bind_keys(cx);

        let bounds = Bounds::centered(None, gpui::size(px(900.), px(760.)), cx);
        let window_bounds = if fullscreen {
            WindowBounds::Fullscreen(bounds)
        } else {
            WindowBounds::Windowed(bounds)
        };

        let markdown = markdown.clone();
        let title: SharedString = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => SharedString::from(name.to_string()),
            None => SharedString::from(path.display().to_string()),
        };
        cx.open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(title),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                let markdown = markdown.clone();
                cx.new(|cx| {
                    let viewer = Viewer::new(&markdown, window, cx);
                    if fullscreen {
                        // gpui просит fullscreen до того, как окно показано,
                        // и WM такой запрос игнорирует — повторяем после кадра.
                        cx.spawn_in(window, async move |_, cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(120))
                                .await;
                            cx.update(|window, _| window.toggle_fullscreen()).ok();
                        })
                        .detach();
                    }
                    viewer
                })
            },
        )
        .expect("не удалось открыть окно");

        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Args, String> {
        parse_args(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn file_without_flags() {
        let a = args(&["doc.md"]).unwrap();
        assert_eq!(a.path, PathBuf::from("doc.md"));
        assert!(!a.fullscreen);
    }

    #[test]
    fn fullscreen_before_or_after_file() {
        assert!(args(&["--fullscreen", "doc.md"]).unwrap().fullscreen);
        let a = args(&["doc.md", "--fullscreen"]).unwrap();
        assert!(a.fullscreen);
        assert_eq!(a.path, PathBuf::from("doc.md"));
    }

    #[test]
    fn missing_file_is_error() {
        assert!(args(&[]).is_err());
        assert!(args(&["--fullscreen"]).is_err());
    }

    #[test]
    fn unknown_flag_is_error() {
        assert!(args(&["--nope", "doc.md"]).is_err());
    }

    #[test]
    fn two_files_is_error() {
        assert!(args(&["a.md", "b.md"]).is_err());
    }
}
