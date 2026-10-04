//! Full-text search through the index Windows already maintains (the same one
//! Explorer's search box uses). Borrowing it means documents can be found by
//! what is written inside them without Matchstick reading, storing or
//! re-indexing a single file itself.

use crate::win::ComGuard;
use windows::core::{BSTR, GUID, HSTRING, PCWSTR, VARIANT};
use windows::Win32::System::Com::{
    CLSIDFromProgID, CoCreateInstance, IDispatch, CLSCTX_INPROC_SERVER, DISPATCH_FLAGS, DISPATCH_METHOD,
    DISPATCH_PROPERTYGET, DISPPARAMS,
};

/// A document whose name or contents matched.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentHit {
    pub path: String,
    pub name: String,
    /// The passage Windows extracted from the document, if any.
    pub summary: String,
}

const CONNECTION: &str = "Provider=Search.CollatorDSO;Extended Properties='Application=Windows';";

/// Longest passage kept per hit; the UI shows a single line.
const SUMMARY_CHARS: usize = 220;

/// Turn free text into a CONTAINS condition: every word must appear, as a
/// prefix. Anything that is not a letter or digit is dropped, which also makes
/// the text safe to embed in the query.
fn contains_condition(query: &str) -> Option<String> {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| format!("\"{}*\"", word))
        .collect();
    if words.is_empty() || query.chars().filter(|c| c.is_alphanumeric()).count() < 3 {
        return None;
    }
    Some(words.join(" AND "))
}

fn build_sql(condition: &str, limit: usize) -> String {
    format!(
        "SELECT TOP {} System.ItemPathDisplay, System.ItemNameDisplay, System.Search.AutoSummary \
         FROM SystemIndex \
         WHERE SCOPE='file:' AND System.ItemType <> 'Directory' AND CONTAINS(*, '{}') \
         ORDER BY System.Search.Rank DESC",
        limit, condition
    )
}

/// One line of readable text from a document passage.
fn tidy_summary(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(SUMMARY_CHARS).collect()
}

/// Call a method or read a property on a late-bound COM object.
unsafe fn invoke(
    object: &IDispatch,
    name: &str,
    flags: DISPATCH_FLAGS,
    mut args: Vec<VARIANT>,
) -> windows::core::Result<VARIANT> {
    let wide_name = HSTRING::from(name);
    let names = [PCWSTR(wide_name.as_ptr())];
    let mut id = 0i32;
    object.GetIDsOfNames(&GUID::zeroed(), names.as_ptr(), 1, 0, &mut id)?;

    // Automation passes arguments last-to-first
    args.reverse();
    let params = DISPPARAMS {
        rgvarg: args.as_mut_ptr(),
        cArgs: args.len() as u32,
        ..Default::default()
    };
    let mut result = VARIANT::default();
    object.Invoke(id, &GUID::zeroed(), 0, flags, &params, Some(&mut result), None, None)?;
    Ok(result)
}

unsafe fn get(object: &IDispatch, name: &str) -> windows::core::Result<VARIANT> {
    invoke(object, name, DISPATCH_PROPERTYGET, Vec::new())
}

fn as_object(value: &VARIANT) -> windows::core::Result<IDispatch> {
    IDispatch::try_from(value)
}

/// A field's text; NULL and non-text values read as empty.
unsafe fn field_text(fields: &IDispatch, index: i32) -> String {
    let read = || -> windows::core::Result<String> {
        let field = as_object(&invoke(fields, "Item", DISPATCH_PROPERTYGET, vec![VARIANT::from(index)])?)?;
        Ok(BSTR::try_from(&get(&field, "Value")?)?.to_string())
    };
    read().unwrap_or_default()
}

unsafe fn run_query(sql: &str) -> windows::core::Result<Vec<ContentHit>> {
    let class_id = CLSIDFromProgID(&HSTRING::from("ADODB.Connection"))?;
    let connection: IDispatch = CoCreateInstance(&class_id, None, CLSCTX_INPROC_SERVER)?;
    invoke(&connection, "Open", DISPATCH_METHOD, vec![VARIANT::from(CONNECTION)])?;

    let rows = (|| -> windows::core::Result<Vec<ContentHit>> {
        let records = as_object(&invoke(&connection, "Execute", DISPATCH_METHOD, vec![VARIANT::from(sql)])?)?;
        let mut hits = Vec::new();
        while !bool::try_from(&get(&records, "EOF")?)? {
            let fields = as_object(&get(&records, "Fields")?)?;
            let path = field_text(&fields, 0);
            if !path.is_empty() {
                hits.push(ContentHit {
                    path,
                    name: field_text(&fields, 1),
                    summary: tidy_summary(&field_text(&fields, 2)),
                });
            }
            invoke(&records, "MoveNext", DISPATCH_METHOD, Vec::new())?;
        }
        Ok(hits)
    })();

    let _ = invoke(&connection, "Close", DISPATCH_METHOD, Vec::new());
    rows
}

/// What people mean by "a document": things written to be read. The Windows
/// index also covers source code and data files, where a word match is almost
/// always a coincidence.
const DOCUMENT_EXTENSIONS: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "md", "rtf", "odt", "ods", "odp", "csv", "eml",
    "msg", "epub", "one",
];

fn is_document(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|ext| DOCUMENT_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
}

/// How many rows to ask Windows for per document wanted, since many of the
/// best-ranked rows are not documents.
const OVERFETCH: usize = 8;

/// Documents whose name or contents contain every word of `query`.
/// Returns nothing if the query is too short or Windows Search is unavailable.
pub fn search(query: &str, limit: usize) -> Vec<ContentHit> {
    let Some(condition) = contains_condition(query) else {
        return Vec::new();
    };
    let _com = ComGuard::new();
    match unsafe { run_query(&build_sql(&condition, limit * OVERFETCH)) } {
        Ok(hits) => hits.into_iter().filter(|hit| is_document(&hit.path)).take(limit).collect(),
        Err(e) => {
            log::warn!("Windows Search query failed: {}", e);
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_requires_every_word_and_strips_punctuation() {
        assert_eq!(contains_condition("tax return"), Some("\"tax*\" AND \"return*\"".to_string()));
        assert_eq!(contains_condition("  O'Brien's  résumé "), Some("\"O*\" AND \"Brien*\" AND \"s*\" AND \"résumé*\"".to_string()));
        // Quotes and SQL punctuation can never reach the query text
        let hostile = contains_condition("x'); DROP TABLE files;--").unwrap();
        assert!(!hostile.contains('\'') && !hostile.contains(';') && !hostile.contains('-'));
    }

    #[test]
    fn short_or_empty_queries_are_not_sent() {
        assert_eq!(contains_condition("ab"), None);
        assert_eq!(contains_condition("  "), None);
        assert_eq!(contains_condition("!!!"), None);
        assert!(search("a", 5).is_empty());
    }

    #[test]
    fn only_documents_count_as_content_matches() {
        assert!(is_document(r"C:\Users\me\Documents\Report.PDF"));
        assert!(is_document(r"C:\Users\me\notes.md"));
        assert!(!is_document(r"C:\Users\me\project\three.module.min.js"));
        assert!(!is_document(r"C:\Users\me\folder"));
    }

    #[test]
    fn summaries_become_one_short_line() {
        assert_eq!(tidy_summary("  line one\r\n\tline   two  "), "line one line two");
        assert_eq!(tidy_summary(&"x".repeat(1000)).chars().count(), SUMMARY_CHARS);
    }

    /// Runs a real query against this machine's Windows Search index. The
    /// index may legitimately hold no match, so only the plumbing is asserted.
    #[test]
    fn queries_the_windows_index_without_error() {
        let _com = ComGuard::new();
        let sql = build_sql(&contains_condition("the").unwrap(), 3);
        let hits = unsafe { run_query(&sql) }.expect("Windows Search query failed");
        assert!(hits.len() <= 3);
        for hit in &hits {
            assert!(!hit.path.is_empty());
            assert!(hit.summary.chars().count() <= SUMMARY_CHARS);
        }
    }
}
