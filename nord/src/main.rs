//  _____  ___
// /__   \/ __\  Tangerine Cat, kaineer@gmail.com
//   / /\/ /
//  / / / /___   github: https://github.com/kaineer
//  \/  \____/   twitter: https://twitter.com/kaineer
//

use std::env;
use std::process::{self, Command};

const URL: &str = "https://www.nordtheme.com/";

const COLORS: &[(&str, &str)] = &[
    ("night.400", "#2e3440"),
    ("night.300", "#3b4252"),
    ("night.200", "#434c5e"),
    ("night.100", "#4c566a"),
    ("snow.100", "#d8dee9"),
    ("snow.200", "#e5e9f0"),
    ("snow.300", "#eceff4"),
    ("frost.100", "#8fbcbb"),
    ("frost.200", "#88c0d0"),
    ("frost.300", "#81a1c1"),
    ("frost.400", "#5e81ac"),
    ("aurora.red", "#bf616a"),
    ("aurora.orange", "#d08770"),
    ("aurora.yellow", "#ebcb8b"),
    ("aurora.green", "#a3be8c"),
    ("aurora.purple", "#b48ead"),
];

fn parse_hex_color(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    (r, g, b)
}

fn ansi_color(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{r};{g};{b}m")
}

fn ansi_reset() -> &'static str {
    "\x1b[0m"
}

fn find_key(key: &str) -> Option<&'static str> {
    COLORS
        .iter()
        .find(|(name, _)| name.contains(key))
        .map(|(name, _)| *name)
}

fn hex_for(name: &str) -> Option<&'static str> {
    COLORS.iter().find(|(n, _)| *n == name).map(|(_, hex)| *hex)
}

fn show_all(filter: Option<&str>, colored: bool) {
    for (name, hex) in COLORS {
        if filter.is_none_or(|f| name.contains(f)) {
            if colored {
                let (r, g, b) = parse_hex_color(hex);
                println!("{name:>15}: {}{hex}{}", ansi_color(r, g, b), ansi_reset());
            } else {
                println!("{name:>15}: {hex}");
            }
        }
    }
}

fn list_keys() {
    for (name, _) in COLORS {
        println!("{name}");
    }
}

fn display_usage() {
    println!(" $ nord help         # print this message");
    println!(" $ nord [ls]         # list all colors");
    println!(" $ nord keys         # list color names");
    println!(" $ nord color dec    # display decimal color");
    println!(" $ nord color [ansi] # display color highlighted");
    println!(" $ nord color -c     # display bare hex color");
}

fn display_color(color: &str, opt: Option<&str>) {
    let Some(found) = find_key(color) else {
        eprintln!("color not found: {color}");
        process::exit(1);
    };
    let hex = hex_for(found).unwrap();
    let (r, g, b) = parse_hex_color(hex);

    match opt {
        Some("dec") => println!("{r} {g} {b}"),
        Some("-c") => println!("{hex}"),
        Some("ansi") | None => {
            print!("{}{hex}{}", ansi_color(r, g, b), ansi_reset());
            println!();
        }
        Some(other) => {
            eprintln!("unknown option: {other}");
            process::exit(1);
        }
    }
}

fn browse() {
    let _ = Command::new("xdg-open").arg(URL).spawn();
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        None | Some("ls") => show_all(args.get(1).map(String::as_str), true),
        Some("lsn") => show_all(args.get(1).map(String::as_str), false),
        Some("keys") => list_keys(),
        Some("help" | "--help") => display_usage(),
        Some("browse" | "bro") => browse(),
        Some(color) => display_color(color, args.get(1).map(String::as_str)),
    }
}
