use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::indexer::IndexEntry;

/// Rank all entries by fuzzy-matching the pattern against their names. If
/// `pattern` is empty, returns entries in insertion order, capped at `limit`.
pub fn rank<'a, I>(entries: I, pattern: &str, limit: usize) -> Vec<&'a IndexEntry>
where
    I: IntoIterator<Item = &'a IndexEntry>,
{
    if pattern.trim().is_empty() {
        return entries.into_iter().take(limit).collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pat = Pattern::parse(pattern, CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, &IndexEntry)> = entries
        .into_iter()
        .filter_map(|e| {
            let haystack = Utf32Str::new(&e.name, &mut buf);
            pat.score(haystack, &mut matcher).map(|s| (s, e))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    scored.truncate(limit);
    scored.into_iter().map(|(_, e)| e).collect()
}
