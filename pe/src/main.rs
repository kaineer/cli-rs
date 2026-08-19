//  _____  ___
// /__   \/ __\  Tangerine Cat, kaineer@gmail.com
//   / /\/ /
//  / / / /___   github: https://github.com/kaineer
//  \/  \____/   twitter: https://twitter.com/kaineer
//
// What: Project environment script
//
// Dependencies:
//  * direnv
//  * EDITOR environment variable
//

use std::env;
use std::fs;
use std::io;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;
use chrono::Local;
use colored::*;

fn get_project_path() -> io::Result<String> {
    match env::var("PROJECT_PATH") {
        Ok(val) => Ok(val),
        Err(_) => {
            let current_dir = env::current_dir()?;
            Ok(current_dir.display().to_string())
        }
    }
}

fn get_project_bin() -> io::Result<String> {
    match env::var("PROJECT_BIN") {
        Ok(val) => Ok(val),
        Err(_) => {
            let project_path = get_project_path()?;
            Ok(format!("{}/bin", project_path))
        }
    }
}

fn print_envrc(project_path: &str) -> io::Result<()> {
    let envrc_path = format!("{}/.envrc", project_path);
    let envrc_path = Path::new(&envrc_path);
    
    if envrc_path.exists() {
        let content = fs::read_to_string(envrc_path)?;
        print!("{}", content);
    } else {
        eprintln!("No .envrc file found in project root");
    }    

    Ok(())
}

fn init_project(project_path: &str, project_name: &str) -> io::Result<()> {
    // Создаем директорию bin
    let bin_path = format!("{}/bin", project_path);
    fs::create_dir_all(&bin_path)?;
    
    // Создаем .envrc если не существует
    let envrc_path = format!("{}/.envrc", project_path);
    let envrc_path = Path::new(&envrc_path);
    if !envrc_path.exists() {
        let content = generate_envrc_content(project_path, project_name)?;
        fs::write(envrc_path, content)?;
    }
    
    // Разрешаем direnv
    let _ = Command::new("direnv")
        .args(&["allow", project_path])
        .output();
    
    // Если DEBUG=true, выводим содержимое
    if env::var("DEBUG").unwrap_or_default() == "true" {
        let content = fs::read_to_string(envrc_path)?;
        print!("{}", content);
    }
    
    Ok(())
}

fn generate_envrc_content(project_path: &str, project_name: &str) -> io::Result<String> {
    let date = Local::now().format("%d.%m.%Y").to_string();
    let bin_path = format!("{}/bin", project_path);
    let node_modules_path = format!("{}/node_modules/.bin", project_path);
    
    Ok(format!(
        "#!/usr/bin/env bash\n\
         \n\
         [ -d {} ] && export PATH={}:$PATH\n\
         [ -d {} ] && export PATH={}:$PATH\n\
         export PROJECT_PATH={}\n\
         export PROJECT_BIN={}/bin\n\
         export PROJECT_NAME={}\n\
         export PROJECT_CREATED=\"{}\"\n\
         export VAR_ROOT=\"$PROJECT_PATH/tmp\"\n",
        bin_path, bin_path,
        node_modules_path, node_modules_path,
        project_path,
        project_path,
        project_name,
        date
    ))
}

fn edit_file(path: &str) -> io::Result<()> {
    let project_path = get_project_path()?;
    let full_path = format!("{}/{}", project_path, path);
    
    let editor = env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    let status = Command::new(editor)
        .arg(&full_path)
        .status()?;
    
    if !status.success() {
        eprintln!("Editor exited with error");
    }
    
    Ok(())
}

fn edit_script(script_name: &str) -> io::Result<()> {
    let project_bin = get_project_bin()?;
    let script_path = format!("{}/{}", project_bin, script_name);
    
    // Создаем пустой файл если не существует
    let path = Path::new(&script_path);
    if !path.exists() {
        fs::write(path, "#!/usr/bin/env bash\n")?;
    }
    
    // Делаем файл исполняемым
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms)?;
    }
    
    // Открываем в редакторе
    let editor = env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    let status = Command::new(editor)
        .arg(&script_path)
        .status()?;
    
    if !status.success() {
        eprintln!("Editor exited with error");
    }
    
    Ok(())
}

fn list_in_bin(option: Option<&String>) -> io::Result<()> {
    let project_bin = get_project_bin()?;
    let bin_path = Path::new(&project_bin);
    
    if bin_path.exists() && bin_path.is_dir() {
        // Выполняем ls с опциями
        let mut cmd = Command::new("ls");
        if let Some(opts) = option {
            cmd.args(opts.split_whitespace());
        }
        cmd.arg(&project_bin);
        
        let output = cmd.output()?;
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
    }
    
    Ok(())
}

fn create_footprint(project_path: &str) -> io::Result<()> {
    let user = env::var("USER").unwrap_or_else(|_| "unknown".to_string());
    let hostname = env::var("HOSTNAME")
        .or_else(|_| {
            let output = Command::new("hostname").output();
            match output {
                Ok(out) if !out.stdout.is_empty() => {
                    String::from_utf8(out.stdout).map(|s| s.trim().to_string())
                }
                _ => Ok("unknown".to_string())
            }
        })
        .unwrap_or_else(|_| "unknown".to_string());
    
    let footprint_dir = format!("{}/config/footprint", project_path);
    fs::create_dir_all(&footprint_dir)?;
    
    let envrc_path = format!("{}/.envrc", project_path);
    let envrc_path = Path::new(&envrc_path);
    if envrc_path.exists() {
        let content = fs::read_to_string(envrc_path)?;
        let filename = format!("{}@{}.envrc", user, hostname);
        let footprint_path = Path::new(&footprint_dir).join(&filename);
        fs::write(&footprint_path, content)?;
        println!("Footprint saved to {}", footprint_path.display());
    } else {
        eprintln!("No .envrc file found in project root");
    }
    
    Ok(())
}

fn print_usage(base: &str) {
    if !std::io::stdout().is_terminal() {
        colored::control::set_override(false);
    }
    
    println!("{}", "Project Environment Manager\n".bold().underline().cyan());
    
    // Макрос для удобства
    macro_rules! line {
        ($cmd:expr, $desc:expr) => {
            println!(" {} {:<10} {:20} {} {}",
                "$".cyan().bold(),
                base.white().bold(),
                $cmd.cyan().bold(),
                "#".bright_black(),
                $desc.bright_black()
            );
        };
    }
    
    line!("h[elp]", "show this text");
    line!("reinit", "init project environment again");
    line!("init", "create bin dir etc");
    line!("ls", "list created scripts");
    line!("e[dit]", "edit .envrc");
    line!("fp|footprint", "store .envrc for current user@host");
    line!("[scriptname]", "create/edit a script");
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let base = args[0].clone();
    
    // Получаем PROJECT_PATH из переменной окружения или текущей директории
    let project_path = get_project_path()?;
    let project_name = Path::new(&project_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    
    if args.len() < 2 {
        print_envrc(&project_path)?;
        return Ok(());
    }
    
    match args[1].as_str() {
        "reinit" => {
            let envrc_path = format!("{}/.envrc", project_path);
            let _ = fs::remove_file(&envrc_path);
            init_project(&project_path, &project_name)?;
        }
        "init" => {
            init_project(&project_path, &project_name)?;
        }
        "erc" => {
            print_envrc(&project_path)?;
        }
        "e" | "edit" => {
            edit_file(".envrc")?;
        }
        "ls" | "list" => {
            list_in_bin(args.get(2))?;
        }
        "paw" | "footprint" | "fp" => {
            create_footprint(&project_path)?;
        }
        "h" | "help" | "-h" | "--help" => {
            print_usage(&base);
        }
        script_name => {
            if !script_name.is_empty() {
                edit_script(script_name)?;
            } else {
                print_envrc(&project_path)?;
            }
        }
    }
    
    Ok(())
}

