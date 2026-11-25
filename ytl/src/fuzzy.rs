//! Простой fzf-подобный фильтр: подпоследовательность символов без учёта регистра.

/// Индексы `items`, прошедшие фильтр, по убыванию «качества» совпадения.
pub fn filter(items: &[String], query: &str) -> Vec<usize> {
    let q: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    if q.is_empty() {
        return (0..items.len()).collect();
    }

    let mut scored: Vec<(i64, usize)> = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        if let Some(score) = score_match(item, &q) {
            scored.push((score, idx));
        }
    }
    // Лучший score первым; при равенстве — исходный порядок.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, i)| i).collect()
}

fn score_match(item: &str, query: &[char]) -> Option<i64> {
    let lower: Vec<char> = item.chars().flat_map(|c| c.to_lowercase()).collect();
    let mut score: i64 = 0;
    let mut last = None::<usize>;
    let mut qi = 0;

    for (i, ch) in lower.iter().enumerate() {
        if qi < query.len() && *ch == query[qi] {
            // Бонус за совпадение в начале / сразу после предыдущего символа.
            if last.map(|p| i == p + 1).unwrap_or(i == 0) {
                score += 8;
            } else {
                score += 2;
            }
            if i == 0 {
                score += 4;
            }
            last = Some(i);
            qi += 1;
            if qi == query.len() {
                // Штраф за длину — короткие заголовки выше.
                score -= lower.len() as i64;
                return Some(score);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_keeps_order() {
        let items = vec!["a".into(), "b".into()];
        assert_eq!(filter(&items, ""), vec![0, 1]);
    }

    #[test]
    fn subsequence_match() {
        let items = vec![
            "Me at the zoo".into(),
            "Zoo Tycoon OST".into(),
            "something else".into(),
        ];
        let hits = filter(&items, "zoo");
        assert_eq!(hits.len(), 2);
        assert!(hits.contains(&0));
        assert!(hits.contains(&1));
        assert!(!hits.contains(&2));
    }
}
