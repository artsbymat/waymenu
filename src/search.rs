use crate::SearchResult;

/// Scores ordered, case-insensitive character matches without allocating per candidate.
pub fn fuzzy_score(candidate: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let candidate_lower = candidate.to_lowercase();
    let query_lower = query.to_lowercase();
    if candidate_lower == query_lower {
        return Some(10_000);
    }
    let mut score = if candidate_lower.starts_with(&query_lower) {
        5_000
    } else {
        0
    };
    let mut chars = candidate_lower.char_indices();
    let mut previous = None;
    for needle in query_lower.chars() {
        let (index, _) = chars.find(|(_, value)| *value == needle)?;
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
    Some(score - candidate_lower.len().min(100) as i64)
}

pub fn filter<'a>(
    items: impl IntoIterator<Item = &'a SearchResult>,
    query: &str,
    limit: usize,
) -> Vec<SearchResult> {
    let mut matches: Vec<_> = items
        .into_iter()
        .filter_map(|item| {
            let title_score = fuzzy_score(&item.title, query);
            let description_score =
                fuzzy_score(&item.description, query).map(|score| score - 1_000);
            title_score
                .into_iter()
                .chain(description_score)
                .max()
                .map(|match_score| {
                    let mut result = item.clone();
                    result.score += match_score;
                    result
                })
        })
        .collect();
    matches.sort_unstable_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    matches.truncate(limit);
    matches
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
