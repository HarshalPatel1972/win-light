use crate::db::{Database, FileEntry};
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::RwLock;
use unicode_segmentation::UnicodeSegmentation;

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
    pub match_type: String, // "exact", "prefix", "substring", "fuzzy", "path"
    /// Matched positions in `filename`, as UTF-16 code unit offsets (what
    /// JavaScript string indexing uses).
    pub matched_indices: Vec<usize>,
    /// For matches inside a document: the passage that matched.
    pub snippet: String,
}

/// An indexed file plus its case-folded forms, precomputed once so that a
/// search never allocates per item.
struct Item {
    entry: FileEntry,
    name_fold: String,
    path_fold: String,
    /// Something the user thinks of as "an app": a Store app, or a shortcut
    /// from the Start Menu or desktop (as opposed to any program file).
    is_app: bool,
    /// How many folders deep the item sits.
    depth: u8,
}

impl Item {
    fn new(entry: FileEntry) -> Self {
        let path_fold = fold(&entry.filepath);
        let is_app = crate::win::is_shell_target(&entry.filepath)
            || (entry.file_type == "shortcut"
                && (path_fold.contains("\\start menu\\") || path_fold.contains("\\desktop\\")));
        Item {
            name_fold: fold(&entry.filename),
            depth: entry.filepath.matches('\\').count().min(255) as u8,
            path_fold,
            is_app,
            entry,
        }
    }

    /// Everything about the item itself (not the query) that makes it a more
    /// or less likely thing to be looking for.
    fn standing(&self, name_matched: bool, now: i64) -> f64 {
        // A launcher's first job is launching: an app whose name matches beats files
        let kind = if self.is_app && name_matched { APP_BOOST } else { file_type_boost(&self.entry.file_type) };

        // People's own files sit near the top of their folders; the depths
        // belong to projects and tools
        let depth_penalty = if self.is_app {
            0.0
        } else {
            (self.depth.saturating_sub(FREE_DEPTH) as f64 * DEPTH_PENALTY).min(MAX_DEPTH_PENALTY)
        };

        // What was touched lately is more likely wanted again
        let age_days = (now - self.entry.modified_at) / 86_400;
        let fresh = match age_days {
            _ if self.entry.modified_at <= 0 => 0.0,
            0..=7 => 20.0,
            8..=30 => 10.0,
            _ => 0.0,
        };

        kind - depth_penalty + fresh + usage_boost(self.entry.click_count, self.entry.last_accessed, now)
    }
}

/// How many fuzzy (guessed) matches a search may show.
const MAX_FUZZY_RESULTS: usize = 3;

/// Added to an app whose name matches the query.
const APP_BOOST: f64 = 300.0;
/// Folder levels (drive included) that cost nothing, and the cost of each one beyond.
const FREE_DEPTH: u8 = 4;
const DEPTH_PENALTY: f64 = 6.0;
const MAX_DEPTH_PENALTY: f64 = 60.0;

/// Lowercase one character at a time, so the folded string always has the same
/// number of characters as the original and match positions carry over.
fn fold(s: &str) -> String {
    s.chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

/// The whole file index held in memory. Searches scan this instead of the
/// database, so a keystroke costs a few milliseconds regardless of what the
/// indexer is doing.
#[derive(Default)]
pub struct SearchIndex {
    items: RwLock<Vec<Item>>,
}

impl SearchIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the contents with the current state of the database.
    pub fn reload(&self, db: &Database) -> Result<usize, String> {
        let entries = db.load_all().map_err(|e| format!("Failed to load index: {}", e))?;
        Ok(self.set_entries(entries))
    }

    fn set_entries(&self, entries: Vec<FileEntry>) -> usize {
        let items: Vec<Item> = entries.into_iter().map(Item::new).collect();
        let count = items.len();
        *self.items.write().unwrap() = items;
        count
    }

    pub fn len(&self) -> usize {
        self.items.read().unwrap().len()
    }

    /// Copies of up to `limit` entries that satisfy `keep`.
    pub fn collect(&self, keep: impl Fn(&FileEntry) -> bool, limit: usize) -> Vec<FileEntry> {
        self.items
            .read()
            .unwrap()
            .iter()
            .map(|item| &item.entry)
            .filter(|entry| keep(entry))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Whether `filepath` is an indexed entry (i.e. something we may launch).
    pub fn contains_path(&self, filepath: &str) -> bool {
        self.items.read().unwrap().iter().any(|i| i.entry.filepath == filepath)
    }

    /// Mirror a click into memory so ranking reflects it immediately.
    pub fn record_click(&self, filepath: &str, now: i64) {
        let mut items = self.items.write().unwrap();
        if let Some(item) = items.iter_mut().find(|i| i.entry.filepath == filepath) {
            item.entry.click_count += 1;
            item.entry.last_accessed = now;
        }
    }
}

/// How a query matched an item, in descending order of quality.
enum Match {
    Exact,
    /// Exact match on the name without its extension.
    Stem,
    Prefix,
    /// Substring of the file name, starting at this byte offset of the folded name.
    Substring(usize),
    Path,
    Fuzzy,
}

/// Fuzzy scores are capped below a substring match so that scattered letters
/// never outrank a literal hit.
const FUZZY_SCORE_CAP: f64 = 550.0;

/// Rank every indexed item against `query` and return the best `max_results`.
pub fn search(index: &SearchIndex, query: &str, max_results: usize) -> Vec<SearchResult> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    let q = fold(query);
    let now = chrono::Utc::now().timestamp();

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::new(query, CaseMatching::Ignore, Normalization::Smart, AtomKind::Fuzzy);
    let mut buf = Vec::new();

    let items = index.items.read().unwrap();
    let mut candidates: Vec<(f64, usize, Match)> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        let name = item.name_fold.as_str();
        let (base, kind) = if name == q {
            (1000.0, Match::Exact)
        } else if name.rsplit_once('.').is_some_and(|(stem, _)| stem == q) {
            (950.0, Match::Stem)
        } else if name.starts_with(&q) {
            (800.0, Match::Prefix)
        } else if let Some(pos) = name.find(&q) {
            (600.0, Match::Substring(pos))
        } else if item.path_fold.contains(&q) {
            (300.0, Match::Path)
        } else if let Some(score) =
            pattern.score(Utf32Str::new(&item.entry.filename, &mut buf), &mut matcher)
        {
            ((score as f64).min(FUZZY_SCORE_CAP), Match::Fuzzy)
        } else {
            continue;
        };

        let name_matched = !matches!(kind, Match::Path | Match::Fuzzy);
        candidates.push((base + item.standing(name_matched, now), idx, kind));
    }

    // Best score first; shorter names win ties.
    candidates.sort_unstable_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| items[a.1].name_fold.len().cmp(&items[b.1].name_fold.len()))
            .then_with(|| a.1.cmp(&b.1))
    });

    // The same file name ten folders apart (ten copies of a library's
    // README) would crowd everything else out, so each name is listed once:
    // its best-ranked copy. Copies the user has actually opened always stay.
    let mut names_listed = HashSet::new();
    candidates.retain(|(_, idx, _)| {
        let item = &items[*idx];
        names_listed.insert(item.name_fold.as_str()) || item.entry.click_count > 0
    });

    // Fuzzy matches are guesses. A few are a safety net for typos; a screenful
    // of them is noise.
    let mut guesses = 0;
    candidates.retain(|(_, _, kind)| !matches!(kind, Match::Fuzzy) || {
        guesses += 1;
        guesses <= MAX_FUZZY_RESULTS
    });
    candidates.truncate(max_results);

    // Highlight positions are only worked out for the results actually returned.
    let query_chars = q.chars().count();
    candidates
        .into_iter()
        .map(|(score, idx, kind)| {
            let item = &items[idx];
            let entry = &item.entry;
            let (match_type, matched_indices) = match kind {
                Match::Exact => ("exact", (0..entry.filename.encode_utf16().count()).collect()),
                Match::Stem => ("exact", chars_to_utf16(&entry.filename, 0, query_chars)),
                Match::Prefix => ("prefix", chars_to_utf16(&entry.filename, 0, query_chars)),
                Match::Substring(pos) => {
                    let start = item.name_fold[..pos].chars().count();
                    ("substring", chars_to_utf16(&entry.filename, start, query_chars))
                }
                Match::Path => ("path", Vec::new()),
                Match::Fuzzy => {
                    let mut indices = Vec::new();
                    pattern.indices(
                        Utf32Str::new(&entry.filename, &mut buf),
                        &mut matcher,
                        &mut indices,
                    );
                    indices.sort_unstable();
                    indices.dedup();
                    ("fuzzy", graphemes_to_utf16(&entry.filename, &indices))
                }
            };

            to_result(entry, score, match_type, matched_indices)
        })
        .collect()
}

/// The items the user opens most often and most recently, best first.
/// Shown on the launcher's home view before anything is typed.
pub fn suggestions(index: &SearchIndex, max_results: usize) -> Vec<SearchResult> {
    let now = chrono::Utc::now().timestamp();
    let items = index.items.read().unwrap();

    let mut used: Vec<(f64, &FileEntry)> = items
        .iter()
        .filter(|item| item.entry.click_count > 0)
        .map(|item| {
            let e = &item.entry;
            (usage_boost(e.click_count, e.last_accessed, now), e)
        })
        .collect();
    used.sort_unstable_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.last_accessed.cmp(&a.1.last_accessed))
    });
    used.truncate(max_results);

    used.into_iter()
        .map(|(score, entry)| to_result(entry, score, "suggestion", Vec::new()))
        .collect()
}

fn to_result(entry: &FileEntry, score: f64, match_type: &str, matched_indices: Vec<usize>) -> SearchResult {
    SearchResult {
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
        match_type: match_type.to_string(),
        matched_indices,
        snippet: String::new(),
    }
}

/// UTF-16 offsets covered by `len` characters of `name` starting at character `start`.
fn chars_to_utf16(name: &str, start: usize, len: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut offset = 0;
    for (i, c) in name.chars().enumerate() {
        let units = c.len_utf16();
        if i >= start + len {
            break;
        }
        if i >= start {
            out.extend(offset..offset + units);
        }
        offset += units;
    }
    out
}

/// UTF-16 offsets covered by the given grapheme clusters of `name`
/// (the unit the fuzzy matcher reports positions in). `graphemes` must be sorted.
fn graphemes_to_utf16(name: &str, graphemes: &[u32]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut wanted = graphemes.iter().peekable();
    let mut offset = 0;
    for (i, g) in name.graphemes(true).enumerate() {
        let units = g.encode_utf16().count();
        if wanted.peek().is_some_and(|&&w| w as usize == i) {
            out.extend(offset..offset + units);
            wanted.next();
        }
        offset += units;
    }
    out
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
fn usage_boost(click_count: i64, last_accessed: i64, now: i64) -> f64 {
    // Click count boost: logarithmic to prevent domination
    let click_boost = if click_count > 0 {
        (click_count as f64).ln() * 15.0
    } else {
        0.0
    };

    // Recency boost: higher for recently accessed items
    let recency_boost = if last_accessed > 0 {
        let age_hours = ((now - last_accessed) as f64 / 3600.0).max(1.0);
        // Decay over time: full boost if accessed in last hour, diminishing after
        (100.0 / age_hours).min(30.0)
    } else {
        0.0
    };

    click_boost + recency_boost
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, filename: &str, file_type: &str) -> FileEntry {
        FileEntry {
            id,
            filename: filename.to_string(),
            filepath: format!(r"C:\files\{}", filename),
            extension: String::new(),
            file_size: 0,
            modified_at: 0,
            file_type: file_type.to_string(),
            click_count: 0,
            last_accessed: 0,
        }
    }

    fn index_of(entries: Vec<FileEntry>) -> SearchIndex {
        let index = SearchIndex::new();
        index.set_entries(entries);
        index
    }

    fn names(results: &[SearchResult]) -> Vec<&str> {
        results.iter().map(|r| r.filename.as_str()).collect()
    }

    /// The characters of `filename` selected by `matched_indices`, the way the
    /// frontend reads them (UTF-16 code units).
    fn highlighted(result: &SearchResult) -> String {
        let units: Vec<u16> = result.filename.encode_utf16().collect();
        let picked: Vec<u16> = result.matched_indices.iter().map(|&i| units[i]).collect();
        String::from_utf16(&picked).unwrap()
    }

    #[test]
    fn ranks_exact_then_prefix_then_substring_then_fuzzy() {
        let index = index_of(vec![
            entry(1, "my notes.txt", "other"),
            entry(2, "notes", "other"),
            entry(3, "notes-2024.txt", "other"),
            entry(4, "n_o_t_e_s.txt", "other"),
            entry(5, "unrelated.txt", "other"),
        ]);

        let results = search(&index, "notes", 10);

        assert_eq!(names(&results), vec!["notes", "notes-2024.txt", "my notes.txt", "n_o_t_e_s.txt"]);
        let kinds: Vec<&str> = results.iter().map(|r| r.match_type.as_str()).collect();
        assert_eq!(kinds, vec!["exact", "prefix", "substring", "fuzzy"]);
    }

    #[test]
    fn name_without_extension_counts_as_exact() {
        let index = index_of(vec![entry(1, "Word.lnk", "shortcut"), entry(2, "Wordpad.lnk", "shortcut")]);
        let results = search(&index, "word", 10);
        assert_eq!(names(&results), vec!["Word.lnk", "Wordpad.lnk"]);
        assert_eq!(results[0].match_type, "exact");
        assert_eq!(highlighted(&results[0]), "Word");
    }

    #[test]
    fn search_is_case_insensitive_and_matches_paths() {
        let index = index_of(vec![entry(1, "Report.PDF", "document")]);
        assert_eq!(search(&index, "REPORT", 10).len(), 1);
        let by_path = search(&index, r"files\rep", 10);
        assert_eq!(by_path.len(), 1);
        assert_eq!(by_path[0].match_type, "path");
    }

    #[test]
    fn multi_word_queries_match_out_of_order_words() {
        let index = index_of(vec![entry(1, "Visual Studio Code.lnk", "shortcut")]);
        assert_eq!(search(&index, "code visual", 10).len(), 1);
    }

    #[test]
    fn apps_and_frequently_used_items_rank_higher() {
        let mut used = entry(2, "chrome-notes.txt", "other");
        used.click_count = 20;
        let index = index_of(vec![
            entry(1, "chrome-backup.txt", "other"),
            used,
            entry(3, "chrome.exe", "app"),
        ]);

        let results = search(&index, "chrome", 10);

        assert_eq!(names(&results), vec!["chrome.exe", "chrome-notes.txt", "chrome-backup.txt"]);
    }

    #[test]
    fn record_click_updates_ranking_immediately() {
        let index = index_of(vec![entry(1, "alpha one.txt", "other"), entry(2, "alpha two.txt", "other")]);
        index.record_click(r"C:\files\alpha two.txt", chrono::Utc::now().timestamp());
        assert_eq!(search(&index, "alpha", 10)[0].filename, "alpha two.txt");
        assert!(index.contains_path(r"C:\files\alpha one.txt"));
        assert!(!index.contains_path(r"C:\files\missing.txt"));
    }

    fn at(id: i64, path: &str, file_type: &str) -> FileEntry {
        let mut e = entry(id, path.rsplit('\\').next().unwrap(), file_type);
        e.filepath = path.to_string();
        e
    }

    #[test]
    fn a_matching_app_beats_files_with_a_closer_name() {
        let index = index_of(vec![
            at(1, r"C:\Users\me\go\src\pow.go", "code"),
            at(2, r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\PowerPoint.lnk", "shortcut"),
            FileEntry { filename: "PowerShell".to_string(), ..at(3, r"shell:AppsFolder\Microsoft.PowerShell!App", "app") },
            at(4, r"C:\Program Files\Tool\powercfg-helper.exe", "app"),
        ]);

        let results = search(&index, "pow", 10);

        // Both real apps come first; the exact-named code file and the bare
        // program file follow.
        let top: HashSet<&str> = results[..2].iter().map(|r| r.filename.as_str()).collect();
        assert_eq!(top, HashSet::from(["PowerPoint.lnk", "PowerShell"]));
        assert_eq!(results[2].filename, "pow.go");
    }

    #[test]
    fn identical_names_are_listed_once_unless_opened() {
        let mut opened = at(3, r"C:\Users\me\Documents\c\README.md", "document");
        opened.click_count = 2;
        let index = index_of(vec![
            at(1, r"C:\Users\me\Documents\a\README.md", "document"),
            at(2, r"C:\Users\me\Documents\b\README.md", "document"),
            opened,
            at(4, r"C:\Users\me\Documents\readme-notes.md", "document"),
        ]);

        let results = search(&index, "readme", 10);

        let paths: Vec<&str> = results.iter().map(|r| r.filepath.as_str()).collect();
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&r"C:\Users\me\Documents\c\README.md"), "the opened copy must stay");
        assert!(paths.contains(&r"C:\Users\me\Documents\readme-notes.md"));
    }

    #[test]
    fn only_a_few_fuzzy_guesses_are_shown() {
        let index = index_of((0..20).map(|i| entry(i, &format!("i_n_v_o_i_c_e_{}.txt", i), "other")).collect());
        let results = search(&index, "invoice", 15);
        assert_eq!(results.len(), MAX_FUZZY_RESULTS);
        assert!(results.iter().all(|r| r.match_type == "fuzzy"));
    }

    #[test]
    fn shallow_files_outrank_deeply_buried_ones() {
        let index = index_of(vec![
            at(1, r"C:\Users\me\Documents\projects\x\lib\deps\inner\budget-old.xlsx", "document"),
            at(2, r"C:\Users\me\Documents\budget-new.xlsx", "document"),
        ]);
        assert_eq!(search(&index, "budget", 10)[0].filename, "budget-new.xlsx");
    }

    #[test]
    fn suggestions_are_the_most_used_items_only() {
        let now = chrono::Utc::now().timestamp();
        let mut often = entry(1, "often.txt", "other");
        often.click_count = 30;
        often.last_accessed = now - 86_400;
        let mut once = entry(2, "once.txt", "other");
        once.click_count = 1;
        once.last_accessed = now - 86_400;
        let index = index_of(vec![once, entry(3, "never.txt", "other"), often]);

        assert_eq!(names(&suggestions(&index, 6)), vec!["often.txt", "once.txt"]);
        assert_eq!(suggestions(&index, 1).len(), 1);
    }

    #[test]
    fn respects_max_results_and_empty_query() {
        let index = index_of((0..50).map(|i| entry(i, &format!("file{}.txt", i), "other")).collect());
        assert_eq!(search(&index, "file", 15).len(), 15);
        assert!(search(&index, "   ", 15).is_empty());
    }

    #[test]
    fn highlights_are_utf16_offsets_for_non_ascii_names() {
        let index = index_of(vec![
            entry(1, "📁 Büro notes.txt", "other"),
            entry(2, "報告書2024.docx", "document"),
            entry(3, "İstanbul.txt", "other"),
        ]);

        // Substring after an emoji (2 UTF-16 units) and a multi-byte letter.
        let r = &search(&index, "notes", 10)[0];
        assert_eq!(r.match_type, "substring");
        assert_eq!(highlighted(r), "notes");

        let r = &search(&index, "büro", 10)[0];
        assert_eq!(highlighted(r), "Büro");

        let r = &search(&index, "報告", 10)[0];
        assert_eq!(r.match_type, "prefix");
        assert_eq!(highlighted(r), "報告");

        // 'İ' lowercases to two characters; positions must not drift.
        let r = &search(&index, "stanbul", 10)[0];
        assert_eq!(highlighted(r), "stanbul");
    }

    #[test]
    fn fuzzy_highlights_point_at_the_matched_letters() {
        let index = index_of(vec![entry(1, "🚀 Größe_Tabelle.xlsx", "document")]);
        let r = &search(&index, "grtab", 10)[0];
        assert_eq!(r.match_type, "fuzzy");
        assert_eq!(highlighted(r).to_lowercase(), "grtab");
    }

    #[test]
    fn test_file_type_boost_values() {
        assert!(file_type_boost("app") > file_type_boost("document"));
        assert!(file_type_boost("document") > file_type_boost("other"));
    }
}
