//! Fuzzy ranking of a list against a typed query, for the dashboard's filter field.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// The items matching `query`, best match first. An empty (or whitespace-only) query keeps every
/// item in its original order.
///
/// `keys` yields the strings one item is matched on; the item keeps its best-scoring key, so a list
/// can be searched by several names at once — a repo by its directory name or its `owner/repo` slug,
/// a topic by its own name or by any task inside it. Items scoring equally keep their input order.
/// Matching is smart-case: an all-lowercase query ignores case, any uppercase letter in it makes
/// case significant.
pub fn rank_by<'a, T, K>(items: &'a [T], query: &str, keys: impl Fn(&'a T) -> K) -> Vec<&'a T>
where
    K: IntoIterator,
    K::Item: AsRef<str>,
{
    let query = query.trim();
    if query.is_empty() {
        return items.iter().collect();
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let case = if query.chars().any(char::is_uppercase) {
        CaseMatching::Respect
    } else {
        CaseMatching::Ignore
    };
    let pattern = Pattern::parse(query, case, Normalization::Smart);

    let mut buf = Vec::new();
    let mut scored: Vec<(u32, &T)> = items
        .iter()
        .filter_map(|item| {
            keys(item)
                .into_iter()
                .filter_map(|key| {
                    pattern.score(Utf32Str::new(key.as_ref(), &mut buf), &mut matcher)
                })
                .max()
                .map(|score| (score, item))
        })
        .collect();
    scored.sort_by(|(a, _), (b, _)| b.cmp(a));
    scored.into_iter().map(|(_, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Item {
        main: &'static str,
        alt: Vec<&'static str>,
    }

    fn item(main: &'static str, alt: &[&'static str]) -> Item {
        Item {
            main,
            alt: alt.to_vec(),
        }
    }

    fn fixtures() -> Vec<Item> {
        vec![
            item("con-1698-databricks", &["con-1711", "con-1779"]),
            item("bugfixes", &["con-1802-flaky-scan"]),
            item("wt_overview", &["wt_ui_filter"]),
        ]
    }

    fn keys(i: &Item) -> Vec<&str> {
        std::iter::once(i.main)
            .chain(i.alt.iter().copied())
            .collect()
    }

    fn mains<'a>(hits: &[&'a Item]) -> Vec<&'a str> {
        hits.iter().map(|i| i.main).collect()
    }

    // Pins that an empty or blank query filters nothing and keeps the input order.
    #[test]
    fn an_empty_query_keeps_everything_in_order() {
        let items = fixtures();
        assert_eq!(
            mains(&rank_by(&items, "", keys)),
            vec!["con-1698-databricks", "bugfixes", "wt_overview"]
        );
        assert_eq!(rank_by(&items, "   ", keys).len(), items.len());
    }

    // Pins that the match is fuzzy: a subsequence of an item's first key finds it.
    #[test]
    fn matches_a_subsequence_of_the_first_key() {
        let items = fixtures();
        assert_eq!(
            mains(&rank_by(&items, "dbricks", keys)),
            vec!["con-1698-databricks"]
        );
    }

    // Pins that an item is found by any of its keys, not only the first — a task inside a
    // group surfaces the group.
    #[test]
    fn matches_any_of_the_later_keys() {
        let items = fixtures();
        assert_eq!(
            mains(&rank_by(&items, "1711", keys)),
            vec!["con-1698-databricks"]
        );
        assert_eq!(mains(&rank_by(&items, "flaky", keys)), vec!["bugfixes"]);
    }

    // Pins that an item is ranked by its best-scoring key, so a strong match on a later key
    // overtakes an item that matched only its first key equally well.
    #[test]
    fn keeps_the_best_scoring_key_per_item() {
        let items = vec![
            item("alpha-scan-con", &["nothing"]),
            item("beta-scan-con", &["con-1711"]),
        ];
        assert_eq!(
            mains(&rank_by(&items, "con", keys)),
            vec!["beta-scan-con", "alpha-scan-con"]
        );
    }

    // Pins smart-case: a lowercase query ignores case, an uppercase letter makes it count.
    #[test]
    fn ignores_case_until_the_query_uses_some() {
        let items = fixtures();
        assert_eq!(rank_by(&items, "bugfixes", keys).len(), 1);
        assert!(rank_by(&items, "BUGFIXES", keys).is_empty());
    }

    // Pins that a query matching nothing empties the list rather than keeping everything.
    #[test]
    fn a_query_nothing_matches_yields_nothing() {
        let items = fixtures();
        assert!(rank_by(&items, "zzzz", keys).is_empty());
    }

    // Pins that an empty key never matches, so a nameless item cannot surface.
    #[test]
    fn an_item_with_no_keys_never_matches() {
        let items = vec![item("con-1698-databricks", &[]), item("", &[])];
        assert_eq!(
            mains(&rank_by(&items, "con", keys)),
            vec!["con-1698-databricks"]
        );
    }
}
