use crate::db::{Database, FileEntry};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A search result with computed score and match metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: i64,
    pub filename: String,
    pub filepath: String,
    pub extension: String,
    pub file_size: i64,
    pub modified_at: i64,
    pub file_type: String,
    pub click_count: i64,
    pub last_accessed: i64,
    pub score: f64,
    pub match_type: String,       // "exact", "prefix", "substring", "fuzzy", "path"
    pub matched_indices: Vec<usize>, // character positions that matched
}

/// Performs multi-strategy search combining SQL pre-filtering with in-memory fuzzy matching.
///
/// Strategy:
/// 1. SQL LIKE query for prefix/substring matches (fast, uses indexes)
/// 2. In-memory fuzzy matching on all filenames for fuzzy results
/// 3. Combine, deduplicate, rank, and return top results
pub fn search(db: &Arc<Database>, query: &str, max_results: usize) -> Result<Vec<SearchResult>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let query_lower = query.to_lowercase();

    // Step 1: Get SQL-based results (prefix + substring matches)
    let sql_results = db
        .search_files(&query_lower, max_results * 3) // over-fetch for ranking
        .map_err(|e| format!("SQL search error: {}", e))?;

    // Step 2: Score SQL results first
    let matcher = SkimMatcherV2::default();
    let mut scored_results: Vec<SearchResult> = Vec::new();
    let mut seen_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();

    // Process SQL results first (these are prefix/substring matches)
    for entry in &sql_results {
        let (score, match_type, indices) = score_entry(entry, &query_lower, &matcher);
        seen_ids.insert(entry.id);
        scored_results.push(SearchResult {
            id: entry.id,
            filename: entry.filename.clone(),
            filepath: entry.filepath.clone(),
            extension: entry.extension.clone(),
            file_size: entry.file_size,
            modified_at: entry.modified_at,
            file_type: entry.file_type.clone(),
            click_count: entry.click_count,
            last_accessed: entry.last_accessed,
            score,
            match_type,
            matched_indices: indices,
        });
    }

    // Step 3: Only do expensive fuzzy scan if SQL didn't return enough good results
    // This avoids loading 100K+ filenames into memory on every keystroke
    if scored_results.len() < max_results {
    let all_files = db
        .get_all_filenames()
        .map_err(|e| format!("Failed to get filenames: {}", e))?;

    for (id, filename, filepath, file_type, click_count, last_accessed, modified_at) in &all_files {
        if seen_ids.contains(id) {
            continue;
        }

        // Fuzzy match against filename
        if let Some(fuzzy_score) = matcher.fuzzy_match(&filename.to_lowercase(), &query_lower) {
            if fuzzy_score > 0 {
                let indices = matcher
                    .fuzzy_indices(&filename.to_lowercase(), &query_lower)
                    .map(|(_, indices)| indices)
                    .unwrap_or_default();

                let base_score = fuzzy_score as f64;
                let type_boost = file_type_boost(file_type);
                let usage_boost = usage_boost(*click_count, *last_accessed);
                let final_score = base_score * 0.5 + type_boost + usage_boost; // fuzzy gets 0.5x weight

                seen_ids.insert(*id);
                scored_results.push(SearchResult {
                    id: *id,
                    filename: filename.clone(),
                    filepath: filepath.clone(),
                    extension: String::new(),
                    file_size: 0,
                    modified_at: *modified_at,
                    file_type: file_type.clone(),
                    click_count: *click_count,
                    last_accessed: *last_accessed,
                    score: final_score,
                    match_type: "fuzzy".to_string(),
                    matched_indices: indices,
                });
            }
        }
    }
    } // end fuzzy scan conditional

    // Sort by score descending
    scored_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Return top N results
    scored_results.truncate(max_results);
    Ok(scored_results)
}

/// Compute a composite score for a FileEntry based on how well it matches the query.
fn score_entry(
    entry: &FileEntry,
    query_lower: &str,
    matcher: &SkimMatcherV2,
) -> (f64, String, Vec<usize>) {
    let filename_lower = entry.filename.to_lowercase();
    let filepath_lower = entry.filepath.to_lowercase();

    let mut best_score: f64 = 0.0;
    let mut match_type = "none".to_string();
    let mut matched_indices: Vec<usize> = Vec::new();

    // Exact match (highest priority)
    if filename_lower == *query_lower {
        best_score = 1000.0;
        match_type = "exact".to_string();
        matched_indices = (0..entry.filename.len()).collect();
    }
    // Exact match without extension
    else if filename_lower.split('.').next().unwrap_or("") == query_lower {
        best_score = 950.0;
        match_type = "exact".to_string();
        matched_indices = (0..query_lower.len()).collect();
    }
    // Prefix match
    else if filename_lower.starts_with(query_lower) {
        best_score = 800.0;
        match_type = "prefix".to_string();
        matched_indices = (0..query_lower.len()).collect();
    }
    // Substring match in filename
    else if let Some(pos) = filename_lower.find(query_lower) {
        best_score = 600.0;
        match_type = "substring".to_string();
        matched_indices = (pos..pos + query_lower.len()).collect();
    }
    // Path match (e.g., searching "docs/proj" matching path)
    else if filepath_lower.contains(query_lower) {
        best_score = 300.0;
        match_type = "path".to_string();
    }
    // Fuzzy match on filename
    else if let Some(fuzzy_result) = matcher.fuzzy_indices(&filename_lower, query_lower) {
        best_score = (fuzzy_result.0 as f64).max(10.0);
        match_type = "fuzzy".to_string();
        matched_indices = fuzzy_result.1;
    }
    // Fuzzy match on filepath
    else if let Some(fuzzy_result) = matcher.fuzzy_indices(&filepath_lower, query_lower) {
        best_score = (fuzzy_result.0 as f64 * 0.5).max(5.0);
        match_type = "path".to_string();
        matched_indices = fuzzy_result.1;
    }

    // Apply boosts
    let type_boost = file_type_boost(&entry.file_type);
    let usage_boost = usage_boost(entry.click_count, entry.last_accessed);

    let final_score = best_score + type_boost + usage_boost;

    (final_score, match_type, matched_indices)
}

/// Boost score based on file type (apps rank higher than documents, etc.)
fn file_type_boost(file_type: &str) -> f64 {
    match file_type {
        "app" => 50.0,
        "shortcut" => 40.0,
        "document" => 20.0,
        "folder" => 15.0,
        "code" => 10.0,
        "image" => 5.0,
        _ => 0.0,
    }
}

/// Boost score based on usage frequency and recency.
fn usage_boost(click_count: i64, last_accessed: i64) -> f64 {
    // Click count boost: logarithmic to prevent domination
    let click_boost = if click_count > 0 {
        (click_count as f64).ln() * 15.0
    } else {
        0.0
    };

    // Recency boost: higher for recently accessed items
    let recency_boost = if last_accessed > 0 {
        let now = chrono::Utc::now().timestamp();
        let age_hours = ((now - last_accessed) as f64 / 3600.0).max(1.0);
        // Decay over time: full boost if accessed in last hour, diminishing after
        (100.0 / age_hours).min(30.0)
    } else {
        0.0
    };

    click_boost + recency_boost
}

/// Evaluate a math expression if the query looks like one.
/// Supports basic arithmetic: +, -, *, /, parentheses.
pub fn evaluate_math(query: &str) -> Option<String> {
    let trimmed = query.trim();

    // Must contain at least one operator and one digit
    if !trimmed.chars().any(|c| c.is_ascii_digit())
        || !trimmed.chars().any(|c| matches!(c, '+' | '-' | '*' | '/' | '%' | '^'))
    {
        return None;
    }

    // Only allow safe characters
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '*' | '/' | '.' | '(' | ')' | ' ' | '%' | '^'))
    {
        return None;
    }

    // Simple recursive descent parser for basic math
    match parse_expression(trimmed) {
        Some((result, rest)) if rest.trim().is_empty() => {
            // Format nicely: remove trailing zeros for whole numbers
            if result.fract() == 0.0 && result.abs() < 1e15 {
                Some(format!("{}", result as i64))
            } else {
                Some(format!("{:.6}", result).trim_end_matches('0').trim_end_matches('.').to_string())
            }
        }
        _ => None,
    }
}

// ---- Simple math expression parser ----

fn parse_expression(input: &str) -> Option<(f64, &str)> {
    let (mut left, mut rest) = parse_term(input)?;
    let mut rest_trimmed = rest.trim_start();
    while rest_trimmed.starts_with('+') || rest_trimmed.starts_with('-') {
        let op = rest_trimmed.chars().next()?;
        let after_op = rest_trimmed[1..].trim_start();
        let (right, new_rest) = parse_term(after_op)?;
        left = match op {
            '+' => left + right,
            '-' => left - right,
            _ => unreachable!(),
        };
        rest = new_rest;
        rest_trimmed = rest.trim_start();
    }
    Some((left, rest))
}

fn parse_term(input: &str) -> Option<(f64, &str)> {
    let (mut left, mut rest) = parse_factor(input)?;
    let mut rest_trimmed = rest.trim_start();
    while rest_trimmed.starts_with('*') || rest_trimmed.starts_with('/') || rest_trimmed.starts_with('%') {
        let op = rest_trimmed.chars().next()?;
        let after_op = rest_trimmed[1..].trim_start();
        let (right, new_rest) = parse_factor(after_op)?;
        left = match op {
            '*' => left * right,
            '/' => {
                if right == 0.0 {
                    return None; // division by zero
                }
                left / right
            }
            '%' => {
                if right == 0.0 {
                    return None;
                }
                left % right
            }
            _ => unreachable!(),
        };
        rest = new_rest;
        rest_trimmed = rest.trim_start();
    }
    Some((left, rest))
}

fn parse_factor(input: &str) -> Option<(f64, &str)> {
    let trimmed = input.trim_start();

    // Handle parentheses
    if trimmed.starts_with('(') {
        let (val, rest) = parse_expression(&trimmed[1..])?;
        let rest = rest.trim_start();
        if rest.starts_with(')') {
            return Some((val, &rest[1..]));
        }
        return None;
    }

    // Handle negative numbers
    if trimmed.starts_with('-') {
        let (val, rest) = parse_factor(&trimmed[1..])?;
        return Some((-val, rest));
    }

    // Parse number
    let mut end = 0;
    let mut has_dot = false;
    for (i, c) in trimmed.char_indices() {
        if c.is_ascii_digit() {
            end = i + 1;
        } else if c == '.' && !has_dot {
            has_dot = true;
            end = i + 1;
        } else {
            break;
        }
    }

    if end == 0 {
        return None;
    }

    let num: f64 = trimmed[..end].parse().ok()?;
    Some((num, &trimmed[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_eval_basic() {
        assert_eq!(evaluate_math("2+2"), Some("4".to_string()));
        assert_eq!(evaluate_math("10 * 5"), Some("50".to_string()));
        assert_eq!(evaluate_math("100 / 4"), Some("25".to_string()));
        assert_eq!(evaluate_math("3.14 * 2"), Some("6.28".to_string()));
    }

    #[test]
    fn test_math_eval_complex() {
        assert_eq!(evaluate_math("(2 + 3) * 4"), Some("20".to_string()));
        assert_eq!(evaluate_math("10 + 5 * 2"), Some("20".to_string()));
        assert_eq!(evaluate_math("100 / (2 + 3)"), Some("20".to_string()));
    }

    #[test]
    fn test_math_eval_invalid() {
        assert_eq!(evaluate_math("hello"), None);
        assert_eq!(evaluate_math(""), None);
        assert_eq!(evaluate_math("abc + 2"), None);
    }

    #[test]
    fn test_math_division_by_zero() {
        assert_eq!(evaluate_math("5 / 0"), None);
    }

    #[test]
    fn test_file_type_boost_values() {
        assert!(file_type_boost("app") > file_type_boost("document"));
        assert!(file_type_boost("document") > file_type_boost("other"));
    }
}
