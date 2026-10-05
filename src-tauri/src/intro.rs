//! Picks the real apps and files shown in the first-run story, so that what
//! emerges from the dark is the user's own PC rather than stock artwork.

use crate::db::FileEntry;
use crate::icons::IconCache;
use crate::indexer;
use crate::searcher::SearchIndex;
use crate::win::is_shell_target;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// How many tiles the story scatters around the match.
const THING_COUNT: usize = 18;
/// How many of those are files and folders rather than apps.
const FILE_COUNT: usize = 6;
/// Rows in the "how it works" demo, and letters typed to find them.
const DEMO_ROWS: usize = 3;
const DEMO_QUERY_CHARS: usize = 3;

/// Start Menu entries nobody would search for.
const BORING: &[&str] = &[
    "uninstall", "readme", "read me", "help", "license", "website", "documentation",
    "manual", "release notes", "setup", "repair", "what's new",
];

#[derive(Debug, Clone, Serialize)]
pub struct IntroItem {
    pub name: String,
    pub file_type: String,
    /// PNG data URL, if the shell has an icon for it.
    pub icon: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Intro {
    pub things: Vec<IntroItem>,
    /// A few letters that find a real app on this PC...
    pub demo_query: String,
    /// ...and the apps they find.
    pub demo_rows: Vec<IntroItem>,
}

#[derive(Debug, Clone, PartialEq)]
struct Candidate {
    name: String,
    path: String,
    file_type: String,
    clicks: i64,
}

/// The name as shown to the user: shortcuts lose their extension.
fn display_name(filename: &str) -> String {
    let lower = filename.to_lowercase();
    if lower.ends_with(".lnk") || lower.ends_with(".url") {
        filename[..filename.len() - 4].to_string()
    } else {
        filename.to_string()
    }
}

/// Start Menu folders full of system utilities: real, but not what anyone
/// thinks of as "my apps".
const SYSTEM_FOLDERS: &[&str] = &["administrative tools", "windows tools", "system tools", "windows powershell"];

fn is_app(entry: &FileEntry) -> bool {
    if !matches!(entry.file_type.as_str(), "app" | "shortcut") {
        return false;
    }
    if is_shell_target(&entry.filepath) {
        return true;
    }
    let path = entry.filepath.to_lowercase();
    path.contains("start menu") && !SYSTEM_FOLDERS.iter().any(|folder| path.contains(folder))
}

fn is_file(entry: &FileEntry) -> bool {
    matches!(entry.file_type.as_str(), "document" | "image" | "folder" | "code")
}

/// Turn entries into candidates: readable names, no clutter, no duplicates,
/// the user's favourites first.
fn candidates(entries: Vec<FileEntry>) -> Vec<Candidate> {
    let mut seen = HashSet::new();
    let mut out: Vec<Candidate> = entries
        .into_iter()
        .map(|e| Candidate {
            name: display_name(&e.filename),
            path: e.filepath,
            file_type: e.file_type,
            clicks: e.click_count,
        })
        .filter(|c| {
            let lower = c.name.to_lowercase();
            !BORING.iter().any(|word| lower.contains(word)) && seen.insert(lower)
        })
        .collect();
    out.sort_by(|a, b| b.clicks.cmp(&a.clicks).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

/// `count` items spread evenly across `items`, so the pick is a cross-section
/// rather than everything starting with "A".
fn spread<T: Clone>(items: &[T], count: usize) -> Vec<T> {
    if items.len() <= count {
        return items.to_vec();
    }
    (0..count).map(|i| items[i * items.len() / count].clone()).collect()
}

/// Choose the tiles: mostly apps, some files, topped up with apps if files
/// are scarce. Returns `count` candidates at most.
fn pick_things(apps: &[Candidate], files: &[Candidate], count: usize) -> Vec<Candidate> {
    let files = spread(files, FILE_COUNT);
    let mut things = spread(apps, count - files.len());
    things.extend(files);
    things
}

/// Keep the items whose icon has not been seen yet. Many
/// shortcuts share the same generic icon; a stage full of blank pages would
/// say nothing, so each picture appears once.
fn distinct_icons(items: impl Iterator<Item = IntroItem>) -> Vec<IntroItem> {
    let mut seen = HashSet::new();
    items
        .filter(|item| item.icon.as_ref().is_some_and(|icon| seen.insert(icon.clone())))
        .collect()
}

/// Choose the demo: the few letters that match the most apps (up to a full
/// list of rows), so typing them visibly "finds" something real.
fn pick_demo(apps: &[Candidate]) -> (String, Vec<Candidate>) {
    let mut by_prefix: HashMap<String, Vec<&Candidate>> = HashMap::new();
    let mut order = Vec::new();
    for app in apps {
        let prefix: String = app.name.chars().take(DEMO_QUERY_CHARS).collect::<String>().to_lowercase();
        if prefix.chars().count() < DEMO_QUERY_CHARS
            || app.name.chars().count() <= DEMO_QUERY_CHARS
            || !prefix.chars().all(char::is_alphanumeric)
        {
            continue;
        }
        let group = by_prefix.entry(prefix.clone()).or_default();
        if group.is_empty() {
            order.push(prefix);
        }
        group.push(app);
    }

    // Different apps beat three entries of one product ("Foo", "Foo Manager",
    // "Foo Store"); then the fullest group wins; among equals, the one seen
    // first (i.e. most used).
    let variety = |group: &[&Candidate]| {
        let first_words: HashSet<String> = group
            .iter()
            .take(DEMO_ROWS)
            .map(|c| c.name.split_whitespace().next().unwrap_or("").to_lowercase())
            .collect();
        first_words.len()
    };
    let Some(best) = order.iter().enumerate().max_by_key(|(position, prefix)| {
        let group = &by_prefix[*prefix];
        (variety(group), group.len().min(DEMO_ROWS), std::cmp::Reverse(*position))
    }).map(|(_, prefix)| prefix) else {
        return (String::new(), Vec::new());
    };
    let rows = by_prefix[best].iter().take(DEMO_ROWS).map(|c| (*c).clone()).collect();
    (best.clone(), rows)
}

fn with_icon(icons: &IconCache, candidate: Candidate) -> IntroItem {
    IntroItem {
        icon: icons.get(&candidate.path),
        name: candidate.name,
        file_type: candidate.file_type,
    }
}

/// Gather what the first-run story shows. On a brand-new install the index is
/// still being built, so apps are read straight from the Start Menu instead.
pub fn build(index: &SearchIndex, icons: &IconCache) -> Intro {
    let mut app_entries = index.collect(is_app, 3000);
    if app_entries.is_empty() {
        app_entries = indexer::quick_app_entries().into_iter().filter(is_app).collect();
    }
    let apps = candidates(app_entries);
    let files = candidates(index.collect(is_file, 400));

    let (demo_query, demo_rows) = pick_demo(&apps);
    // Over-pick, since items with a duplicate or missing icon are dropped
    let pool = pick_things(&apps, &files, THING_COUNT * 3);
    Intro {
        things: spread(&distinct_icons(pool.into_iter().map(|c| with_icon(icons, c))), THING_COUNT),
        demo_query,
        demo_rows: demo_rows.into_iter().map(|c| with_icon(icons, c)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(filename: &str, file_type: &str, clicks: i64) -> FileEntry {
        FileEntry {
            id: 0,
            filename: filename.to_string(),
            filepath: format!(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\{}", filename),
            extension: String::new(),
            file_size: 0,
            modified_at: 0,
            file_type: file_type.to_string(),
            click_count: clicks,
            last_accessed: 0,
        }
    }

    fn names(items: &[Candidate]) -> Vec<&str> {
        items.iter().map(|c| c.name.as_str()).collect()
    }

    #[test]
    fn candidates_are_cleaned_deduplicated_and_favourites_first() {
        let found = candidates(vec![
            entry("Zeta.lnk", "shortcut", 0),
            entry("Uninstall Zeta.lnk", "shortcut", 0),
            entry("Alpha.lnk", "shortcut", 0),
            entry("alpha.url", "shortcut", 0),
            entry("Mail", "app", 9),
        ]);
        assert_eq!(names(&found), vec!["Mail", "Alpha", "Zeta"]);
    }

    #[test]
    fn things_are_a_spread_of_apps_and_files() {
        let apps = candidates((0..100).map(|i| entry(&format!("App {:03}.lnk", i), "shortcut", 0)).collect());
        let files = candidates((0..3).map(|i| entry(&format!("doc{}.pdf", i), "document", 0)).collect());

        let things = pick_things(&apps, &files, THING_COUNT);

        assert_eq!(things.len(), THING_COUNT);
        assert_eq!(things.iter().filter(|c| c.file_type == "document").count(), 3);
        assert_eq!(things[0].name, "App 000");
        assert!(things.iter().any(|c| c.name.as_str() > "App 080"), "should sample the whole range");
    }

    #[test]
    fn demo_uses_the_letters_that_find_the_most_apps() {
        let apps = candidates(vec![
            entry("Calculator", "app", 0),
            entry("Photos", "app", 0),
            entry("Photoshop.lnk", "shortcut", 0),
            entry("Phone Link", "app", 0),
            entry("7-Zip.lnk", "shortcut", 0),
            entry("Go", "app", 0),
        ]);

        let (query, rows) = pick_demo(&apps);

        assert_eq!(query, "pho");
        assert_eq!(names(&rows), vec!["Phone Link", "Photos", "Photoshop"]);
    }

    /// A fresh install has an empty index; the story must still find real apps.
    #[test]
    fn builds_from_the_apps_on_this_machine_before_any_index_exists() {
        let intro = build(&SearchIndex::new(), &IconCache::default());
        assert!(!intro.things.is_empty(), "no apps found in the Start Menu");
        assert!(intro.things.iter().all(|thing| thing.icon.is_some()), "tiles without icons");
    }

    #[test]
    fn demo_prefers_different_apps_over_one_product_family() {
        let apps = candidates(vec![
            entry("BlueStacks 5.lnk", "shortcut", 0),
            entry("BlueStacks Manager.lnk", "shortcut", 0),
            entry("BlueStacks Store.lnk", "shortcut", 0),
            entry("Steam.lnk", "shortcut", 0),
            entry("Stellarium.lnk", "shortcut", 0),
        ]);
        assert_eq!(pick_demo(&apps).0, "ste");
    }

    #[test]
    fn each_icon_appears_once_and_missing_icons_are_skipped() {
        let item = |name: &str, icon: Option<&str>| IntroItem {
            name: name.to_string(),
            file_type: "app".to_string(),
            icon: icon.map(str::to_string),
        };
        let kept = distinct_icons(
            vec![item("a", Some("X")), item("b", Some("X")), item("c", None), item("d", Some("Y")), item("e", Some("Z"))]
                .into_iter(),
        );
        let kept: Vec<&str> = kept.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(kept, vec!["a", "d", "e"]);
    }

    #[test]
    fn demo_is_empty_when_there_is_nothing_suitable() {
        let (query, rows) = pick_demo(&candidates(vec![entry("Go", "app", 0)]));
        assert!(query.is_empty());
        assert!(rows.is_empty());
    }
}
