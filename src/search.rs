use crate::SearchResult;

/// Scores ordered, case-insensitive character matches without allocating per candidate.
pub fn fuzzy_score(candidate: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let query_lower = query.to_lowercase();
    fuzzy_score_lowercased(candidate, query, &query_lower)
}

/// Internal helper that avoids re-allocating lowercased query for each candidate.
pub fn fuzzy_score_lowercased(candidate: &str, query_raw: &str, query_lower: &str) -> Option<i64> {
    if query_lower.is_empty() {
        return Some(0);
    }
    if candidate.eq_ignore_ascii_case(query_raw) {
        return Some(10_000);
    }

    if candidate.is_ascii() && query_lower.is_ascii() {
        let cand_bytes = candidate.as_bytes();
        let query_bytes = query_lower.as_bytes();
        let starts_with_prefix = cand_bytes.len() >= query_bytes.len()
            && cand_bytes[..query_bytes.len()].eq_ignore_ascii_case(query_bytes);

        let mut score = if starts_with_prefix { 5_000 } else { 0 };
        let mut cand_pos = 0usize;
        let mut previous = None;

        for &needle in query_bytes {
            let offset = cand_bytes[cand_pos..]
                .iter()
                .position(|&b| b.to_ascii_lowercase() == needle)?;
            let index = cand_pos + offset;
            cand_pos = index + 1;
            score += 100;
            if let Some(last) = previous {
                let gap = index.saturating_sub(last + 1) as i64;
                score -= gap.min(50);
                if gap == 0 {
                    score += 25;
                }
            } else {
                score -= index.min(50) as i64;
            }
            previous = Some(index);
        }

        let cand_char_count = cand_bytes.len();
        return Some(score - cand_char_count.min(100) as i64);
    }

    let starts_with_prefix = {
        let mut cand_chars = candidate.chars().flat_map(char::to_lowercase);
        let query_chars = query_lower.chars();
        let mut matches = true;
        for q in query_chars {
            if cand_chars.next() != Some(q) {
                matches = false;
                break;
            }
        }
        matches
    };

    let mut score = if starts_with_prefix { 5_000 } else { 0 };
    let mut chars = candidate.char_indices();
    let mut previous = None;
    let mut cand_char_count = 0usize;

    for needle in query_lower.chars() {
        let (index, _) = chars.find(|(_, value)| {
            cand_char_count += 1;
            value.to_ascii_lowercase() == needle || value.to_lowercase().any(|c| c == needle)
        })?;
        score += 100;
        if let Some(last) = previous {
            let gap = index.saturating_sub(last + 1) as i64;
            score -= gap.min(50);
            if gap == 0 {
                score += 25;
            }
        } else {
            score -= index.min(50) as i64;
        }
        previous = Some(index);
    }

    for _ in chars {
        cand_char_count += 1;
    }

    Some(score - cand_char_count.min(100) as i64)
}

pub fn filter<'a>(
    items: impl IntoIterator<Item = &'a SearchResult>,
    query: &str,
    limit: usize,
) -> Vec<SearchResult> {
    if limit == 0 {
        return Vec::new();
    }
    let query = query.trim();
    if query.is_empty() {
        return items.into_iter().take(limit).cloned().collect();
    }
    let query_lower = query.to_lowercase();
    let mut matches: Vec<(&'a SearchResult, i64)> = Vec::new();

    for item in items {
        let title_score = fuzzy_score_lowercased(&item.title, query, &query_lower);
        let match_score = match title_score {
            Some(s1) if s1 >= 10_000 => Some(s1),
            title_score => {
                let description_score = if item.description.is_empty() {
                    None
                } else {
                    fuzzy_score_lowercased(&item.description, query, &query_lower)
                        .map(|score| score - 1_000)
                };
                match (title_score, description_score) {
                    (Some(s1), Some(s2)) => Some(s1.max(s2)),
                    (Some(s1), None) => Some(s1),
                    (None, Some(s2)) => Some(s2),
                    (None, None) => None,
                }
            }
        };

        if let Some(match_score) = match_score {
            matches.push((item, item.score + match_score));
        }
    }

    if matches.len() > limit {
        matches.select_nth_unstable_by(limit, |(item_a, score_a), (item_b, score_b)| {
            score_b
                .cmp(score_a)
                .then_with(|| item_a.title.cmp(&item_b.title))
        });
        matches.truncate(limit);
    }

    matches.sort_unstable_by(|(item_a, score_a), (item_b, score_b)| {
        score_b
            .cmp(score_a)
            .then_with(|| item_a.title.cmp(&item_b.title))
    });

    matches
        .into_iter()
        .map(|(item, score)| {
            let mut result = item.clone();
            result.score = score;
            result
        })
        .collect()
}

pub fn filter_strings<'a, I>(items: I, query: &str, limit: usize) -> Vec<SearchResult>
where
    I: IntoIterator<Item = &'a String>,
{
    if limit == 0 {
        return Vec::new();
    }
    let query = query.trim();
    if query.is_empty() {
        return items
            .into_iter()
            .take(limit)
            .enumerate()
            .map(|(index, title)| SearchResult {
                id: index.to_string(),
                title: title.clone(),
                description: String::new(),
                score: 0,
                icon: None,
                image: None,
                metadata: None,
            })
            .collect();
    }

    let query_lower = query.to_lowercase();
    let mut matches: Vec<(&'a str, i64, usize)> = Vec::new();

    for (index, item) in items.into_iter().enumerate() {
        if let Some(score) = fuzzy_score_lowercased(item, query, &query_lower) {
            matches.push((item.as_str(), score, index));
        }
    }

    if matches.len() > limit {
        matches.select_nth_unstable_by(limit, |(title_a, score_a, _), (title_b, score_b, _)| {
            score_b.cmp(score_a).then_with(|| title_a.cmp(title_b))
        });
        matches.truncate(limit);
    }

    matches.sort_unstable_by(|(title_a, score_a, _), (title_b, score_b, _)| {
        score_b.cmp(score_a).then_with(|| title_a.cmp(title_b))
    });

    matches
        .into_iter()
        .map(|(title, score, index)| SearchResult {
            id: index.to_string(),
            title: title.to_owned(),
            description: String::new(),
            score,
            icon: None,
            image: None,
            metadata: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn exact_and_prefix_rank_first() {
        assert!(fuzzy_score("fire", "fire") > fuzzy_score("firefox", "fire"));
        assert!(fuzzy_score("firefox", "fire") > fuzzy_score("not firefox", "fire"));
        assert_eq!(fuzzy_score("abc", "acx"), None);
    }

    #[test]
    fn description_alias_is_searchable_but_lower_ranked() {
        let direct = SearchResult {
            id: "1".into(),
            title: "Writer".into(),
            description: String::new(),
            score: 0,
            icon: None,
            image: None,
            metadata: None,
        };
        let alias = SearchResult {
            id: "2".into(),
            title: "Office".into(),
            description: "Writer word processor".into(),
            score: 0,
            icon: None,
            image: None,
            metadata: None,
        };
        let result = filter([&alias, &direct], "writer", 2);
        assert_eq!(
            result
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
    }

    #[test]
    fn filters_five_thousand_entries_within_budget() {
        let entries: Vec<_> = (0..5_000)
            .map(|index| SearchResult {
                id: index.to_string(),
                title: format!("Application number {index}"),
                description: format!("Utility category {index}"),
                score: 0,
                icon: None,
                image: None,
                metadata: None,
            })
            .collect();
        let start = Instant::now();
        let results = filter(entries.iter(), "apnr42", 20);
        let elapsed = start.elapsed();
        assert!(!results.is_empty());
        let limit = if cfg!(debug_assertions) {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(10)
        };
        assert!(
            elapsed < limit,
            "5,000-entry query took {elapsed:?}, budget {limit:?}"
        );
    }
}
