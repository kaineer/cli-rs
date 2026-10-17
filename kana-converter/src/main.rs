use romaji::RomajiExt;
use std::collections::HashMap;

// Встраиваем YAML словарь прямо в бинарник
const DICT_YAML: &str = include_str!("../data/reading_to_kanji.yaml");

type ReadingToKanji = HashMap<String, Vec<String>>;

struct Dictionary {
    by_reading: ReadingToKanji,
}

impl Dictionary {
    /// Загрузка словаря из встроенных данных
    pub fn from_embedded() -> anyhow::Result<Self> {
        let by_reading: ReadingToKanji = serde_yaml::from_str(DICT_YAML)?;
        Ok(Dictionary { by_reading })
    }

    /// Поиск точного совпадения чтения
    pub fn search_exact(&self, reading: &str) -> Vec<&String> {
        self.by_reading
            .get(reading)
            .map(|kanjis| kanjis.iter().collect())
            .unwrap_or_default()
    }
}

/// Преобразование пунктуации (можно использовать отдельно)
fn convert_punctuation(text: &str) -> String {
    let mut result = String::with_capacity(text.len());

    for c in text.chars() {
        let converted = match c {
            ',' => '、',
            '.' => '。',
            '[' => '「',
            ']' => '」',
            '{' => '『',
            '}' => '』',
            '!' => '！',
            '?' => '？',
            ';' => '；',
            ':' => '：',
            '\'' => '＇',
            _ => c,
        };
        result.push(converted);
    }

    result
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Ошибка: не указана входная строка");
        eprintln!("Использование: kana-converter <ромадзи>");
        std::process::exit(1);
    }

    let input = &args[1];

    let input_with_punct = convert_punctuation(input);

    // Конвертируем ромадзи в хирагану
    let hiragana = input_with_punct.to_hiragana();

    // Выводим первую строку: хирагана
    println!("{}", hiragana);

    // Выводим вторую строку: катакана
    println!("{}", input_with_punct.to_katakana());

    // Загружаем встроенный словарь и ищем кандзи по чтению
    match Dictionary::from_embedded() {
        Ok(dict) => {
            for kanji in dict.search_exact(&hiragana) {
                println!("{}", kanji);
            }
        }
        Err(_e) => {
            // Молча игнорируем ошибки словаря, но для отладки можно раскомментировать:
            // eprintln!("Ошибка загрузки словаря: {}", e);
        }
    }

    Ok(())
}
