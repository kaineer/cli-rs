use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, List, ListItem, Padding, Paragraph},
};

use crate::app::App;

pub fn render_normal(frame: &mut Frame, app: &App) {
    let area = frame.size();

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Black))
        .padding(Padding {
            left: 0,
            right: 0,
            top: 0,
            bottom: 0,
        });

    let inner_area = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    // Динамически определяем количество строк в верхней части
    let mut constraints = vec![Constraint::Length(1)]; // Строка ввода

    if app.has_buffer() {
        constraints.push(Constraint::Length(1)); // Разделитель после ввода
        constraints.push(Constraint::Length(1)); // Строка буфера
        constraints.push(Constraint::Length(1)); // Разделитель после буфера
    }

    constraints.push(Constraint::Min(1)); // Список результатов

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .margin(0)
        .split(inner_area);

    let mut chunk_index = 0;

    // 1. Строка ввода с курсором
    let input_prefix = if app.is_loading() { " ⏳ " } else { " > " };
    let cursor_char = if app.is_loading() { '░' } else { ' ' };
    let cursor_pos = app.input().len();

    let mut display_text = format!("{}{}", input_prefix, app.input());
    display_text.push(cursor_char);

    let input_style = if app.is_loading() {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::White)
    };

    let input_paragraph = Paragraph::new(Span::styled(display_text, input_style));
    frame.render_widget(input_paragraph, chunks[chunk_index]);

    let cursor_x = chunks[chunk_index].x + (input_prefix.len() + cursor_pos) as u16;
    let cursor_y = chunks[chunk_index].y;
    frame.set_cursor(cursor_x, cursor_y);

    chunk_index += 1;

    // 2. Если есть буфер, показываем его
    if app.has_buffer() {
        // Разделитель после ввода
        let separator1 = "─".repeat(chunks[chunk_index].width as usize);
        let separator1_style = Style::default().fg(Color::DarkGray);
        let separator1_paragraph = Paragraph::new(Span::styled(separator1, separator1_style));
        frame.render_widget(separator1_paragraph, chunks[chunk_index]);
        chunk_index += 1;

        // Строка буфера
        let buffer_prefix = if app.is_buffer_selected() {
            " > "
        } else {
            " >>"
        };
        let buffer_display = format!("{}{}", buffer_prefix, app.buffer());
        let buffer_style = if app.is_buffer_selected() {
            Style::default()
                .fg(Color::Green)
                .bg(Color::Rgb(0, 0, 0))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        };

        let buffer_paragraph = Paragraph::new(Span::styled(buffer_display, buffer_style));
        frame.render_widget(buffer_paragraph, chunks[chunk_index]);
        chunk_index += 1;

        // Разделитель после буфера
        let separator2 = "─".repeat(chunks[chunk_index].width as usize);
        let separator2_style = Style::default().fg(Color::DarkGray);
        let separator2_paragraph = Paragraph::new(Span::styled(separator2, separator2_style));
        frame.render_widget(separator2_paragraph, chunks[chunk_index]);
        chunk_index += 1;
    }

    // 3. Список результатов
    if app.has_results() {
        let items: Vec<ListItem> = app
            .script_output()
            .iter()
            .enumerate()
            .map(|(i, line)| {
                // let display_line = if line.is_empty() { " " } else { line.as_str() };
                let display_line = if line.is_empty() {
                    format!(" ")
                } else {
                    format!(" {}", line)
                };

                if Some(i) == app.selected_index() {
                    ListItem::new(display_line).style(
                        Style::default()
                            .fg(Color::White)
                            .bg(Color::Rgb(0, 0, 0))
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    ListItem::new(display_line).style(Style::default().fg(Color::White))
                }
            })
            .collect();

        let list = List::new(items)
            .highlight_style(Style::default().bg(Color::Yellow).fg(Color::Black))
            .highlight_symbol("> ");

        frame.render_widget(list, chunks[chunk_index]);
    } else {
        let hint = if app.is_loading() {
            "⏳ Loading..."
        } else if app.input().is_empty() {
            "Type something and wait 500ms to see results..."
        } else {
            "No results - try typing something else..."
        };

        let hint_style = if app.is_loading() {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let hint_paragraph = Paragraph::new(Span::styled(hint, hint_style));
        frame.render_widget(hint_paragraph, chunks[chunk_index]);
    }
}
