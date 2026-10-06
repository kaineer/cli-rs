use std::io::ErrorKind;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

pub async fn run_script(script_name: &str, input: &str) -> (Vec<String>, String) {
    let cmd = format!("{} {}", script_name, input);

    // Проверяем, есть ли данные в stdin (для пайпов)
    let mut child = if atty::is(atty::Stream::Stdin) {
        // Нет пайпа - используем аргумент как обычно
        match Command::new(script_name)
            .arg(input)
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                return (
                    vec![format!("Error: Failed to execute script - {}", e)],
                    cmd,
                );
            }
        }
    } else {
        // Есть пайп - читаем stdin и передаем в скрипт
        let mut stdin_data = String::new();
        let mut stdin_reader = tokio::io::stdin();

        // Читаем все данные из stdin
        if let Err(e) = stdin_reader.read_to_string(&mut stdin_data).await {
            if e.kind() != ErrorKind::UnexpectedEof {
                eprintln!("Error reading stdin: {}", e);
            }
        }

        match Command::new(script_name)
            .arg(input)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                // Передаем данные из stdin в процесс
                if let Some(mut stdin) = child.stdin.take() {
                    if let Err(e) = stdin.write_all(stdin_data.as_bytes()).await {
                        eprintln!("Error writing to script stdin: {}", e);
                    }
                    if let Err(e) = stdin.flush().await {
                        eprintln!("Error flushing script stdin: {}", e);
                    }
                }
                child
            }
            Err(e) => {
                return (
                    vec![format!("Error: Failed to execute script - {}", e)],
                    cmd,
                );
            }
        }
    };

    // Читаем stdout скрипта
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => return (vec!["Error: Failed to get script output".to_string()], cmd),
    };

    let reader = BufReader::new(stdout);
    let mut lines = Vec::new();

    let mut line_reader = reader.lines();
    while let Ok(Some(line)) = line_reader.next_line().await {
        lines.push(line);
    }

    let _ = child.wait().await;
    (lines, cmd)
}
