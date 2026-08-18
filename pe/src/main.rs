use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use chrono::Local;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let base = args[0].clone();
    
    // Получаем текущую директорию
    let current_dir = env::current_dir()?;
    let project_path = current_dir.display().to_string();
    let project_name = current_dir.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    
    if args.len() < 2 {
        print_envrc(&project_path, &project_name)?;
        return Ok(());
    }
    
    match args[1].as_str() {
        "reinit" => {
            let _ = fs::remove_file(".envrc");
            init_project(&project_path, &project_name)?;
        }
        "init" => {
            init_project(&project_path, &project_name)?;
        }
        "erc" => {
            print_envrc(&project_path, &project_name)?;
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
                print_envrc(&project_path, &project_name)?;
            }
        }
    }
    
    Ok(())
}

fn print_envrc(project_path: &str, project_name: &str) -> io::Result<()> {
    let date = Local::now().format("%d.%m.%Y").to_string();
    let bin_path = format!("{}/bin", project_path);
    let node_modules_path = format!("{}/node_modules/.bin", project_path);
    
    println!("#!/usr/bin/env bash");
    println!();
    println!("[ -d {} ] && export PATH={}:$PATH", bin_path, bin_path);
    println!("[ -d {} ] && export PATH={}:$PATH", node_modules_path, node_modules_path);
    println!("export PROJECT_PATH={}", project_path);
    println!("export PROJECT_BIN={}/bin", project_path);
    println!("export PROJECT_NAME={}", project_name);
    println!("export PROJECT_CREATED=\"{}\"", date);
    println!("export VAR_ROOT=\"$PROJECT_PATH/tmp\"");
    
    Ok(())
}

fn init_project(project_path: &str, project_name: &str) -> io::Result<()> {
    // Создаем директорию bin
    fs::create_dir_all("bin")?;
    
    // Создаем .envrc если не существует
    let envrc_path = Path::new(".envrc");
    if !envrc_path.exists() {
        let content = generate_envrc_content(project_path, project_name)?;
        fs::write(envrc_path, content)?;
    }
    
    // Разрешаем direnv
    let _ = Command::new("direnv")
        .args(&["allow", "."])
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
    let editor = env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    let status = Command::new(editor)
        .arg(path)
        .status()?;
    
    if !status.success() {
        eprintln!("Editor exited with error");
    }
    
    Ok(())
}

fn edit_script(script_name: &str) -> io::Result<()> {
    // Получаем PROJECT_BIN из переменной окружения или используем стандартный путь
    let project_bin = match env::var("PROJECT_BIN") {
        Ok(val) => val,
        Err(_) => {
            let current_dir = env::current_dir()?;
            format!("{}/bin", current_dir.display())
        }
    };
    
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
    edit_file(&script_path)?;
    
    Ok(())
}

fn list_in_bin(option: Option<&String>) -> io::Result<()> {
    // Получаем PROJECT_BIN из переменной окружения или используем стандартный путь
    let project_bin = match env::var("PROJECT_BIN") {
        Ok(val) => val,
        Err(_) => {
            let current_dir = env::current_dir()?;
            format!("{}/bin", current_dir.display())
        }
    }; 
    
    let bin_path = Path::new(&project_bin);
    if bin_path.exists() && bin_path.is_dir() {
        // Переключаемся в директорию bin
        let original_dir = env::current_dir()?;
        env::set_current_dir(bin_path)?;
        
        // Выполняем ls с опциями
        let mut cmd = Command::new("ls");
        if let Some(opts) = option {
            cmd.args(opts.split_whitespace());
        }
        
        let output = cmd.output()?;
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        
        // Возвращаемся обратно
        env::set_current_dir(original_dir)?;
    }
    
    Ok(())
}

fn create_footprint(project_path: &str) -> io::Result<()> {
    let user = env::var("USER").unwrap_or_else(|_| "unknown".to_string());
    let hostname = env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
    
    let footprint_dir = format!("{}/config/footprint", project_path);
    fs::create_dir_all(&footprint_dir)?;
    
    let envrc_path = Path::new(".envrc");
    if envrc_path.exists() {
        let content = fs::read_to_string(envrc_path)?;
        let footprint_path = format!("{}/{}.envrc", footprint_dir, user);
        fs::write(&footprint_path, content)?;
        println!("Footprint saved to {}", footprint_path);
    } else {
        eprintln!("No .envrc file found in project root");
    }
    
    Ok(())
}

fn print_usage(base: &str) {
    println!(" $ {} h[elp] -- show this text", base);
    println!(" $ {} reinit -- init project environment again", base);
    println!(" $ {} init -- create bin dir etc", base);
    println!(" $ {} ls -- list created scripts", base);
    println!(" $ {} e[dit] -- edit .envrc", base);
    println!(" $ {} footprint|fp -- store .envrc for current user@host", base);
    println!(" $ {} [scriptname] -- create/edit a script", base);
}
