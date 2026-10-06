use super::script;
use crate::app::App;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub async fn handle_key(app: &mut App, key: KeyEvent) -> Result<(), Box<dyn std::error::Error>> {
    // Не обрабатываем клавиши во время загрузки
    if app.is_loading() {
        return Ok(());
    }

    // Проверяем, выбран ли буфер
    let is_buffer_selected = app.is_buffer_selected();

    // Отладка: выводим информацию о нажатой клавише в debug режиме
    if app.debug_mode {
        eprintln!(
            "Key pressed: {:?} with modifiers: {:?}",
            key.code, key.modifiers
        );
    }

    match (key.code, key.modifiers) {
        // Обычные символы - добавляем ввод (всегда в input, даже если выбран буфер)
        (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
            // Если был выбран буфер, снимаем выделение с него при начале ввода
            if is_buffer_selected {
                app.select_buffer(false);
            }
            app.push_char(c);
        }

        // Backspace - зависит от того, что выбрано
        (KeyCode::Backspace, KeyModifiers::NONE) => {
            if is_buffer_selected && app.has_buffer() {
                // Если выбран буфер - удаляем из буфера
                app.pop_buffer_char();
                // Если буфер стал пустым, снимаем выделение
                if !app.has_buffer() {
                    app.select_buffer(false);
                }
            } else {
                // Иначе удаляем из input
                app.pop_char();
            }
        }

        // Ctrl+Backspace - очистка последнего символа буфера (всегда буфер)
        (KeyCode::Backspace, KeyModifiers::CONTROL) => {
            if app.has_buffer() {
                app.pop_buffer_char();
                if !app.has_buffer() {
                    app.select_buffer(false);
                }
            }
        }

        // Ctrl+J или Ctrl+Enter
        (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
            handle_ctrl_j(app).await?;
        }

        // Явная проверка на Ctrl+Enter (некоторые терминалы)
        (KeyCode::Enter, KeyModifiers::CONTROL) => {
            handle_ctrl_j(app).await?; // То же поведение, что и Ctrl+J
        }

        // Обычный Enter
        (KeyCode::Enter, KeyModifiers::NONE) => {
            handle_enter(app).await?;
        }

        // Стрелка вниз или Ctrl+N - следующий элемент
        (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
            handle_navigation_down(app);
        }

        // Стрелка вверх или Ctrl+P - предыдущий элемент
        (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
            handle_navigation_up(app);
        }

        // Esc - выход или сброс выбора
        (KeyCode::Esc, _) => {
            if app.has_selection() && !is_buffer_selected {
                // Если выбран вариант - сбрасываем выбор
                app.clear_selection();
            } else if is_buffer_selected {
                // Если выбран буфер - снимаем выделение с буфера
                app.select_buffer(false);
            } else {
                // Иначе выходим
                app.quit();
            }
        }

        _ => {}
    }
    Ok(())
}

fn handle_navigation_down(app: &mut App) {
    let is_buffer_selected = app.is_buffer_selected();
    let has_buffer = app.has_buffer();
    let has_results = app.has_results();
    let has_selection = app.has_selection();
    let has_input = app.has_input();

    if is_buffer_selected {
        // Если выбран буфер - переходим к первому варианту (если есть)
        if has_results {
            app.select_buffer(false);
            app.select_first();
        }
        // Если вариантов нет, остаемся на буфере
    } else if has_results {
        if has_input {
            if has_buffer {
                app.select_buffer(false);
            }

            if has_selection {
                app.select_next();
            } else {
                app.select_first();
            }
        } else if has_buffer {
            app.select_buffer(true);
        }
    } else if has_buffer && !is_buffer_selected {
        // Если результатов нет, но буфер не пуст и не выбран - выбираем буфер
        app.select_buffer(true);
    }
    // Если ничего нет - ничего не делаем
}

fn handle_navigation_up(app: &mut App) {
    let is_buffer_selected = app.is_buffer_selected();
    let has_buffer = app.has_buffer();
    let has_results = app.has_results();
    let selected_index = app.selected_index();

    if is_buffer_selected {
        // Если выбран буфер - остаемся на буфере (он первый)
        // Можно добавить переход в конец списка, но пока ничего
        return;
    }

    if has_buffer && selected_index == Some(0) {
        // Если на первом варианте и есть буфер - переходим на буфер
        app.select_buffer(true);
    } else if has_results {
        // Обычная навигация по вариантам
        app.select_previous();
    } else if has_buffer && !has_results {
        // Если результатов нет, но буфер есть - выбираем буфер
        app.select_buffer(true);
    }
}

async fn handle_ctrl_j(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let is_buffer_selected = app.is_buffer_selected();

    if is_buffer_selected && app.has_buffer() {
        // Если выбран буфер - выводим буфер в stdout
        handle_buffer_enter(app).await?;
    } else if !is_buffer_selected && app.has_results() {
        // Если выбран вариант - добавляем в буфер
        handle_add_to_buffer(app).await?;
    } else if !app.has_results() && !app.is_loading() {
        // Если результатов нет - принудительно загружаем
        handle_force_load(app).await?;
    }
    Ok(())
}

async fn handle_enter(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let is_buffer_selected = app.is_buffer_selected();

    if is_buffer_selected && app.has_buffer() {
        // Если выбран буфер - выводим буфер в stdout
        handle_buffer_enter(app).await?;
    } else if !is_buffer_selected && app.has_results() {
        // Если выбран вариант - выводим вариант в stdout
        handle_selection_enter(app).await?;
    } else if app.input().is_empty() && app.has_buffer() {
        // Если input пуст и есть буфер - выводим буфер в stdout
        handle_buffer_enter(app).await?;
    } else if !app.has_results() && !app.is_loading() {
        // Если результатов нет - принудительно загружаем
        handle_force_load(app).await?;
    }
    Ok(())
}

async fn handle_selection_enter(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(selected) = app.select_current() {
        app.selected_output = Some(selected);
        app.quit();
    }
    Ok(())
}

async fn handle_add_to_buffer(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(selected) = app.select_current() {
        // Добавляем в буфер
        app.append_to_buffer(&selected);

        // Очищаем строку ввода
        app.clear_input();

        // Снимаем выделение с варианта
        app.clear_selection();

        // Очищаем результаты поиска
        app.clear_results();

        // Сбрасываем pending update для нового поиска
        app.set_pending_update(false);

        // Снимаем выделение с буфера, чтобы можно было сразу печатать
        app.select_buffer(false);
    }
    Ok(())
}

async fn handle_buffer_enter(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    if app.has_buffer() {
        let output = app.buffer().replace('\n', "");
        app.selected_output = Some(output);
        app.quit();
    }
    Ok(())
}

async fn handle_force_load(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    app.set_loading(true);
    let input = app.input().to_string();
    let script_name = app.script_name().to_string();

    let (new_output, cmd) = script::run_script(&script_name, &input).await;

    app.update_results(new_output, cmd);
    app.set_loading(false);
    app.set_pending_update(false);

    if app.has_results() {
        app.select_first();
    }
    Ok(())
}
