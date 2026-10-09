// Search filters: `key:value` tokens mixed into a search query, e.g.
// `cargo tag:rust is:due updated:7d`. The filters narrow the result; whatever
// is left is the full-text query. A token whose key is unknown stays text, so
// a search for `std::mem` still works.

use crate::models::{PageMeta, PageSource};

const DAY_MS: i64 = 86_400_000;

/// Help shown to agents and in the UI.
pub const FILTER_HELP: &str = "Filters: `tag:rust` (exact tag), `folder:rust` (folder and its subfolders), \
`tool:claude` (source tool contains), `is:due`, `is:scheduled`, `is:unscheduled`, `is:network`, \
`has:note`, `has:source`, `due:7d` (due within 7 days), `updated:7d`, `created:30d` (in the last N days). \
Prefix a filter with `-` to exclude, and quote values with spaces: `tag:\"machine learning\"`.";

#[derive(Debug, Clone, PartialEq)]
enum Filter {
    Tag(String),
    Folder(String),
    Tool(String),
    Due,
    Scheduled,
    Network,
    HasNote,
    HasSource,
    DueWithin(i64),
    UpdatedWithin(i64),
    CreatedWithin(i64),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Query {
    /// The full-text part, filters removed.
    pub text: String,
    /// `(negated, filter)`.
    filters: Vec<(bool, Filter)>,
}

/// Split a query into tokens, keeping `"quoted text"` (after a `key:` or on
/// its own) together. Quotes are kept on free text so phrase search still works.
fn tokens(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in query.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                cur.push(c);
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `7d`, `2w`, `12h` or a bare number of days, in milliseconds.
fn duration(value: &str) -> Option<i64> {
    let value = value.trim().to_ascii_lowercase();
    let (num, unit) = match value.chars().last()? {
        'd' | 'w' | 'h' => (&value[..value.len() - 1], value.chars().last()?),
        _ => (value.as_str(), 'd'),
    };
    let n: i64 = num.parse().ok().filter(|n| *n >= 0)?;
    let unit_ms = match unit {
        'h' => DAY_MS / 24,
        'w' => DAY_MS * 7,
        _ => DAY_MS,
    };
    n.checked_mul(unit_ms)
}

fn unquote(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
        .to_string()
}

impl Query {
    pub fn parse(query: &str) -> Result<Self, String> {
        let mut text = Vec::new();
        let mut filters = Vec::new();
        for token in tokens(query) {
            let (negated, body) = match token.strip_prefix('-') {
                Some(rest) if rest.contains(':') => (true, rest),
                _ => (false, token.as_str()),
            };
            let Some((key, raw)) = body.split_once(':') else {
                text.push(token);
                continue;
            };
            let value = unquote(raw);
            let bad = || format!("invalid search filter `{token}`. {FILTER_HELP}");
            let filter = match key.to_ascii_lowercase().as_str() {
                "tag" if !value.is_empty() => Filter::Tag(value),
                "folder" if !value.is_empty() => {
                    Filter::Folder(value.trim_matches('/').to_string())
                }
                "tool" if !value.is_empty() => Filter::Tool(value.to_lowercase()),
                "is" => match value.to_ascii_lowercase().as_str() {
                    "due" => Filter::Due,
                    "scheduled" => Filter::Scheduled,
                    "unscheduled" => {
                        filters.push((!negated, Filter::Scheduled));
                        continue;
                    }
                    "network" => Filter::Network,
                    _ => return Err(bad()),
                },
                "has" => match value.to_ascii_lowercase().as_str() {
                    "note" => Filter::HasNote,
                    "source" => Filter::HasSource,
                    _ => return Err(bad()),
                },
                "due" => Filter::DueWithin(duration(&value).ok_or_else(bad)?),
                "updated" => Filter::UpdatedWithin(duration(&value).ok_or_else(bad)?),
                "created" => Filter::CreatedWithin(duration(&value).ok_or_else(bad)?),
                "tag" | "folder" | "tool" => return Err(bad()),
                // Not a filter (e.g. `std::mem`, `http://…`): search it as text.
                _ => {
                    text.push(token);
                    continue;
                }
            };
            filters.push((negated, filter));
        }
        Ok(Query {
            text: text.join(" "),
            filters,
        })
    }

    pub fn has_filters(&self) -> bool {
        !self.filters.is_empty()
    }

    /// Whether `meta` passes every filter at time `now` (unix ms).
    pub fn matches(&self, meta: &PageMeta, now: i64) -> bool {
        self.filters
            .iter()
            .all(|(negated, f)| filter_matches(f, meta, now) != *negated)
    }
}

fn filter_matches(filter: &Filter, meta: &PageMeta, now: i64) -> bool {
    match filter {
        Filter::Tag(tag) => meta.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)),
        Filter::Folder(want) => meta.folder.as_deref().is_some_and(|have| {
            have.eq_ignore_ascii_case(want)
                || have
                    .to_lowercase()
                    .starts_with(&format!("{}/", want.to_lowercase()))
        }),
        Filter::Tool(tool) => PageSource::of(meta)
            .and_then(|s| s.tool)
            .is_some_and(|t| t.to_lowercase().contains(tool.as_str())),
        Filter::Due => meta.next_review.is_some_and(|n| n <= now),
        Filter::Scheduled => meta.next_review.is_some(),
        Filter::Network => meta.allow_cdn,
        Filter::HasNote => !meta.note.trim().is_empty(),
        Filter::HasSource => PageSource::of(meta).is_some(),
        Filter::DueWithin(ms) => meta
            .next_review
            .is_some_and(|n| n <= now.saturating_add(*ms)),
        Filter::UpdatedWithin(ms) => meta.updated_at >= now.saturating_sub(*ms),
        Filter::CreatedWithin(ms) => meta.created_at >= now.saturating_sub(*ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(id: &str) -> PageMeta {
        let mut m = PageMeta::new(id.into());
        m.created_at = 1_000 * DAY_MS;
        m.updated_at = 1_000 * DAY_MS;
        m
    }

    #[test]
    fn splits_filters_from_text() {
        let q =
            Query::parse(r#"cargo lock tag:"machine learning" -is:due "exact phrase""#).unwrap();
        assert_eq!(q.text, r#"cargo lock "exact phrase""#);
        assert_eq!(
            q.filters,
            vec![
                (false, Filter::Tag("machine learning".into())),
                (true, Filter::Due)
            ]
        );
        let q = Query::parse("std::mem http://x.y -flag").unwrap();
        assert_eq!(q.text, "std::mem http://x.y -flag");
        assert!(!q.has_filters());
    }

    #[test]
    fn rejects_bad_values_of_known_filters() {
        for bad in ["is:nope", "has:x", "due:soon", "updated:-3d", "tag:"] {
            let err = Query::parse(bad).unwrap_err();
            assert!(err.contains(bad), "{err}");
        }
        assert_eq!(duration("2w"), Some(14 * DAY_MS));
        assert_eq!(duration("12h"), Some(DAY_MS / 2));
        assert_eq!(duration("3"), Some(3 * DAY_MS));
    }

    #[test]
    fn matches_each_filter() {
        let now = 1_000 * DAY_MS;
        let mut m = meta("a");
        m.tags = vec!["Rust".into()];
        m.folder = Some("lang/rust".into());
        m.next_review = Some(now + 3 * DAY_MS);
        m.note = "n".into();
        PageSource::set(
            &mut m,
            Some(PageSource {
                tool: Some("Claude Code".into()),
                ..Default::default()
            }),
        );

        let yes = |q: &str| Query::parse(q).unwrap().matches(&m, now);
        assert!(yes("tag:rust folder:lang folder:lang/rust tool:claude"));
        assert!(!yes("folder:lan"), "a folder prefix is not a parent folder");
        assert!(yes(
            "is:scheduled -is:due due:7d has:note has:source updated:1d"
        ));
        assert!(!yes("due:2d") && !yes("is:unscheduled") && !yes("is:network"));
        assert!(yes("-tag:python created:1d"));
    }
}
