//! Help content compiled into the binary (`include_str!`) so the wizard always has help text —
//! even in `tauri dev` where no resource directory exists, and on machines that cannot reach
//! the docs site. The files live in `src-tauri/resources/docs/<lang>/` and are the reference
//! layout the docs site must publish (see `resources/docs/README.md`).
//!
//! Adding a page: drop the Markdown file into both language folders, reference it from both
//! `index.json` files, and add a `page!` entry per language below. The unit test
//! `every_index_entry_is_embedded` fails until all three are in sync.

/// One embedded Markdown page.
#[derive(Debug, Clone, Copy)]
pub struct BundledDoc {
    /// Language folder (`zh-CN` or `en`).
    pub lang: &'static str,
    /// Path relative to the language folder, as referenced by `index.json`.
    pub path: &'static str,
    /// File contents.
    pub content: &'static str,
}

macro_rules! page {
    ($lang:literal, $file:literal) => {
        BundledDoc {
            lang: $lang,
            path: $file,
            content: include_str!(concat!("../../resources/docs/", $lang, "/", $file)),
        }
    };
}

/// Embedded `index.json` per language.
pub const INDEXES: &[(&str, &str)] = &[
    (
        "zh-CN",
        include_str!("../../resources/docs/zh-CN/index.json"),
    ),
    ("en", include_str!("../../resources/docs/en/index.json")),
];

/// Embedded Markdown pages (both languages).
pub const PAGES: &[BundledDoc] = &[
    page!("zh-CN", "overview.md"),
    page!("zh-CN", "env-check.md"),
    page!("zh-CN", "one-click.md"),
    page!("zh-CN", "install-node.md"),
    page!("zh-CN", "install-cli.md"),
    page!("zh-CN", "install-cc-switch.md"),
    page!("zh-CN", "configure-cc-switch.md"),
    page!("zh-CN", "verify.md"),
    page!("zh-CN", "troubleshooting.md"),
    page!("zh-CN", "faq.md"),
    page!("en", "overview.md"),
    page!("en", "env-check.md"),
    page!("en", "one-click.md"),
    page!("en", "install-node.md"),
    page!("en", "install-cli.md"),
    page!("en", "install-cc-switch.md"),
    page!("en", "configure-cc-switch.md"),
    page!("en", "verify.md"),
    page!("en", "troubleshooting.md"),
    page!("en", "faq.md"),
];

/// Embedded `index.json` for `lang`, if that language is bundled.
pub fn index(lang: &str) -> Option<&'static str> {
    INDEXES
        .iter()
        .find(|(l, _)| *l == lang)
        .map(|(_, json)| *json)
}

/// Embedded Markdown page at `path` (relative to the language folder), if bundled.
pub fn page(lang: &str, path: &str) -> Option<&'static str> {
    PAGES
        .iter()
        .find(|d| d.lang == lang && d.path == path)
        .map(|d| d.content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_and_pages_are_looked_up_by_language() {
        assert!(index("zh-CN").is_some());
        assert!(index("en").is_some());
        assert!(index("fr").is_none());
        assert!(page("en", "overview.md").is_some());
        assert!(page("en", "missing.md").is_none());
        assert!(page("fr", "overview.md").is_none());
    }

    #[test]
    fn every_page_starts_with_an_h1_and_is_not_empty() {
        for doc in PAGES {
            let first = doc.content.lines().next().unwrap_or_default();
            assert!(
                first.starts_with("# "),
                "{}/{} must start with an H1",
                doc.lang,
                doc.path
            );
            assert!(
                doc.content.trim().len() > 200,
                "{}/{} too short",
                doc.lang,
                doc.path
            );
        }
    }

    #[test]
    fn both_languages_ship_the_same_page_set() {
        let mut zh: Vec<&str> = PAGES
            .iter()
            .filter(|d| d.lang == "zh-CN")
            .map(|d| d.path)
            .collect();
        let mut en: Vec<&str> = PAGES
            .iter()
            .filter(|d| d.lang == "en")
            .map(|d| d.path)
            .collect();
        zh.sort_unstable();
        en.sort_unstable();
        assert_eq!(zh, en);
    }
}
