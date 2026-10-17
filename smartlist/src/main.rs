mod app;
mod handlers;
mod ui;
mod utils;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::CrosstermBackend;
use ratatui::{Terminal, backend::Backend};
use std::time::{Duration, Instant};

use app::App;
use handlers::key::handle_key;
use ui::render;

fn usage(program: &str) -> String {
    format!(
        "\
Использование: {program} [-d] [-q|--query <запрос>] <скрипт>

  <скрипт>     скрипт, который нужно использовать, чтобы получить список для выбора
  -d           отладочный вывод и дополнительный TUI
  -q, --query  первое значение ввода
  --help       показать это сообщение
"
    )
}

fn exit_with_usage(program: &str) -> ! {
    eprint!("{}", usage(program));
    std::process::exit(1);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Получаем аргументы командной строки
    let args: Vec<String> = std::env::args().collect();

    // Парсим флаги
    let mut script_name = None;
    let mut debug_mode = false;
    let mut initial_query = String::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-d" => {
                debug_mode = true;
                i += 1;
            }
            "-q" | "--query" => {
                if i + 1 < args.len() {
                    initial_query = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("Error: -q/--query requires a value");
                    exit_with_usage(&args[0]);
                }
            }
            "--help" => {
                print!("{}", usage(&args[0]));
                std::process::exit(0);
            }
            _ => {
                if script_name.is_none() {
                    script_name = Some(args[i].clone());
                    i += 1;
                } else {
                    exit_with_usage(&args[0]);
                }
            }
        }
    }

    let script_name = match script_name {
        Some(name) => name,
        None => exit_with_usage(&args[0]),
    };

    // Проверяем, доступен ли TUI (должен быть терминал для stderr)
    if !atty::is(atty::Stream::Stderr) {
        eprintln!(
            "Error: TUI mode requires a terminal (stderr is not a terminal).\n\
            \r       Hint: Run without redirecting stderr, or use a terminal emulator.\n\
            \r       Current command: {}",
            args.join(" ")
        );
        std::process::exit(1);
    }

    // Инициализация TUI в stderr
    enable_raw_mode()?;
    let mut stderr = std::io::stderr();
    execute!(stderr, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;

    // Создаем приложение
    let mut app = App::new(script_name, debug_mode, initial_query);

    // Запускаем основной цикл
    let result = run_app(&mut terminal, &mut app).await;

    // Восстанавливаем терминал перед выходом
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Выводим результат в stdout без завершающего переноса строки
    if let Some(selected) = app.selected_output {
        print!("{}", selected);
    }

    if let Err(err) = result {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_update_check = Instant::now();

    while !app.should_quit() {
        terminal.draw(|f| render(f, app))?;

        // Проверяем, не прошло ли достаточно времени для обновления
        if app.pending_update()
            && !app.is_loading()
            && last_update_check.elapsed() > Duration::from_millis(100)
        {
            if app.last_input_time.elapsed() > Duration::from_millis(app::INPUT_DEBOUNCE_MS) {
                app.set_loading(true);
                let input = app.input().to_string();
                let script_name = app.script_name().to_string();

                // Запускаем скрипт
                let (new_output, cmd) = handlers::script::run_script(&script_name, &input).await;

                app.update_results(new_output, cmd);
                app.set_loading(false);
                app.set_pending_update(false);
            }
            last_update_check = Instant::now();
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                handle_key(app, key).await?;
            }
        }
    }
    Ok(())
}
