//! Site rules: how a profile treats each site, set in the extension or in the app's Profiles ›
//! Sites. The extension does the taming and redirecting. The app keeps them in the profile, which
//! syncs, and makes sure a change during a Hard session only makes things stricter.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How a site behaves during a session: left alone, tamed with its recipe, redirected to a page
/// you can get through with a pass, or restricted with no way through. In order of strictness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SiteLevel {
    Allowed,
    Tamed,
    Redirected,
    Restricted,
}

/// A different level for some pages of a site, e.g. redirecting YouTube's /shorts while the
/// rest of YouTube is tamed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PathRule {
    /// "/" is the home page only. Any other path also covers everything under it, and `*`
    /// matches within one part of the path. Matching ignores case.
    pub path: String,
    pub level: SiteLevel,
}

/// A part of a site's page someone picked to hide while it's tamed, with the extension's element
/// picker: the CSS selector that finds it, and a short name for it, like "Trending now".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HiddenPart {
    pub selector: String,
    pub label: String,
}

/// How one site is treated by a profile. Built-in sites go by their recipe's id, like "reddit";
/// ones people add themselves by their address, like "site:example.com".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SiteRule {
    pub recipe_id: String,
    pub level: SiteLevel,
    /// Overrides for the recipe's toggle defaults, keyed by toggle id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, as = "Option<std::collections::HashMap<String, bool>>")]
    pub toggles: Option<BTreeMap<String, bool>>,
    /// Pages that don't follow the site's level. The most specific match wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub paths: Option<Vec<PathRule>>,
}

/// What's set for a site outside any profile: the parts of its page picked to hide, which are
/// hidden whenever it's tamed (in any profile's session, or all the time), and whether it's
/// tamed all the time, even with no session running, with toggles of its own for then.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SiteSettings {
    pub recipe_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hidden: Option<Vec<HiddenPart>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub all_the_time: Option<bool>,
    /// Overrides for the recipe's toggle defaults while it's tamed all the time, kept while it's
    /// off so turning it back on brings them back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, as = "Option<std::collections::HashMap<String, bool>>")]
    pub toggles: Option<BTreeMap<String, bool>>,
}

/// The most sites a profile keeps, and page rules and toggles a site keeps, so a bad sync can't
/// make each check slow. The same as the extension's limits, with room to spare.
const MAX_SITES: usize = 200;
const MAX_PATHS: usize = 50;
const MAX_PATH_LENGTH: usize = 200;
const MAX_TOGGLES: usize = 50;
const MAX_HIDDEN: usize = 50;
const MAX_SELECTOR_LENGTH: usize = 500;
const MAX_LABEL_LENGTH: usize = 80;

/// Drops site rules that can't be right (an id that isn't a site, a page path that isn't a
/// path, a hidden part whose selector could break out of the stylesheet), and anything past the
/// limits.
pub fn tidy(sites: Vec<SiteRule>) -> Vec<SiteRule> {
    let mut sites: Vec<SiteRule> = sites.into_iter().filter(|site| is_site_id(&site.recipe_id)).collect();
    sites.truncate(MAX_SITES);
    for site in &mut sites {
        if let Some(paths) = &mut site.paths {
            paths.retain(|page| {
                page.path.starts_with('/') && page.path.len() <= MAX_PATH_LENGTH && page.path.is_ascii()
            });
            paths.truncate(MAX_PATHS);
        }
        if let Some(toggles) = &mut site.toggles {
            while toggles.len() > MAX_TOGGLES {
                toggles.pop_last();
            }
        }
    }
    sites
}

/// Drops site settings that can't be right (an id that isn't a site, a site listed twice, a
/// hidden part whose selector could break out of the stylesheet), and anything past the limits.
pub fn tidy_settings(settings: Vec<SiteSettings>) -> Vec<SiteSettings> {
    let mut seen = std::collections::HashSet::new();
    let mut settings: Vec<SiteSettings> = settings
        .into_iter()
        .filter(|site| is_site_id(&site.recipe_id) && seen.insert(site.recipe_id.clone()))
        .collect();
    settings.truncate(MAX_SITES);
    for site in &mut settings {
        if let Some(hidden) = &mut site.hidden {
            hidden.retain(|part| part.selector.len() <= MAX_SELECTOR_LENGTH && is_safe_selector(&part.selector));
            hidden.truncate(MAX_HIDDEN);
            for part in hidden.iter_mut() {
                if let Some((end, _)) = part.label.char_indices().nth(MAX_LABEL_LENGTH) {
                    part.label.truncate(end);
                }
            }
        }
        if site.hidden.as_ref().is_some_and(Vec::is_empty) {
            site.hidden = None;
        }
        if let Some(toggles) = &mut site.toggles {
            while toggles.len() > MAX_TOGGLES {
                toggles.pop_last();
            }
        }
    }
    settings
}

/// Adds a backup's site settings to these: parts hidden in either are hidden, and a site tamed
/// all the time here keeps its own toggles. Returns whether anything changed.
pub fn merge_settings(here: &mut Vec<SiteSettings>, theirs: Vec<SiteSettings>) -> bool {
    let before = here.clone();
    for site in tidy_settings(theirs) {
        match here.iter_mut().find(|s| s.recipe_id == site.recipe_id) {
            None => here.push(site),
            Some(mine) => {
                let hidden = mine.hidden.get_or_insert_with(Vec::new);
                for part in site.hidden.into_iter().flatten() {
                    if !hidden.iter().any(|p| p.selector == part.selector) {
                        hidden.push(part);
                    }
                }
                if hidden.is_empty() {
                    mine.hidden = None;
                }
                if mine.all_the_time.is_none() {
                    mine.all_the_time = site.all_the_time;
                    mine.toggles = mine.toggles.take().or(site.toggles);
                }
            }
        }
    }
    *here = tidy_settings(std::mem::take(here));
    *here != before
}

/// Whether a selector can go in the extension's stylesheet without closing its rule or starting
/// another one. The extension checks the same (`isSafeSelector` in packages/core/src/recipe.ts).
fn is_safe_selector(selector: &str) -> bool {
    !selector.trim().is_empty() && !selector.contains(['{', '}', ';', '@', '\\']) && !selector.contains("/*")
}

/// A built-in recipe's id ("reddit"), or "site:" and an address ("site:example.com").
fn is_site_id(id: &str) -> bool {
    let word = |part: &str| {
        !part.is_empty()
            && part.len() <= 63
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    match id.strip_prefix("site:") {
        None => word(id),
        Some(domain) => {
            let labels: Vec<&str> = domain.split('.').collect();
            domain.len() <= 253
                && labels.len() >= 2
                && labels.iter().all(|label| word(label))
                && !labels.last().is_some_and(|tld| tld.bytes().all(|b| b.is_ascii_digit()))
        }
    }
}

/// A profile's sites, and what it does with every site it doesn't list ("Every other site").
#[derive(Clone, Copy, Debug)]
pub struct SiteList<'a> {
    pub sites: &'a [SiteRule],
    pub others: SiteLevel,
}

/// What a profile does with every site it doesn't list, from what's saved: redirected or
/// restricted, or else allowed. Tamed isn't one, since there's no recipe to tame them with. The
/// extension reads it the same way (`otherSitesLevel` in packages/core/src/profiles.ts).
pub fn other_sites(saved: Option<SiteLevel>) -> SiteLevel {
    match saved {
        Some(level @ (SiteLevel::Redirected | SiteLevel::Restricted)) => level,
        _ => SiteLevel::Allowed,
    }
}

/// Whether `new` lets through anything `old` kept away: a site or page at a looser level, one
/// taken off (so it's treated like every other site), one added at a looser level than every
/// other site, or every other site loosened. A new page rule can't be looser than anything
/// already set on its site. Not allowed during a Hard session. The extension checks the same
/// (packages/core/src/sites.ts).
pub fn looser(old: SiteList, new: SiteList) -> bool {
    if new.others < old.others {
        return true;
    }
    let same_path = |a: &PathRule, b: &PathRule| a.path.eq_ignore_ascii_case(&b.path);
    let mut ids: Vec<&str> = old
        .sites
        .iter()
        .chain(new.sites)
        .map(|s| s.recipe_id.as_str())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter().any(|id| {
        let before = old.sites.iter().find(|site| site.recipe_id == id);
        let kept = new.sites.iter().find(|site| site.recipe_id == id);
        // A site that isn't listed is treated like every other site, with no page rules.
        let old_level = before.map_or(old.others, |site| site.level);
        let level = kept.map_or(new.others, |site| site.level);
        let old_pages = before.and_then(|site| site.paths.as_deref()).unwrap_or_default();
        let pages = kept.and_then(|site| site.paths.as_deref()).unwrap_or_default();

        let page_loosened = old_pages.iter().any(|page| {
            let now = pages.iter().find(|p| same_path(p, page)).map_or(level, |p| p.level);
            now < page.level
        });
        let strictest = old_pages.iter().map(|page| page.level).fold(old_level, SiteLevel::max);
        let new_page_looser = pages
            .iter()
            .any(|page| !old_pages.iter().any(|p| same_path(p, page)) && page.level < strictest);
        level < old_level || page_loosened || new_page_looser
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use SiteLevel::*;

    fn list(sites: &[SiteRule]) -> SiteList<'_> {
        allowlist(sites, Allowed)
    }

    fn allowlist(sites: &[SiteRule], others: SiteLevel) -> SiteList<'_> {
        SiteList { sites, others }
    }

    fn site(id: &str, level: SiteLevel, paths: &[(&str, SiteLevel)]) -> SiteRule {
        SiteRule {
            recipe_id: id.into(),
            level,
            toggles: None,
            paths: (!paths.is_empty()).then(|| {
                paths
                    .iter()
                    .map(|&(path, level)| PathRule {
                        path: path.into(),
                        level,
                    })
                    .collect()
            }),
        }
    }

    #[test]
    fn reads_what_the_extension_saves_and_writes_it_back_the_same() {
        let saved = r#"[{"recipeId":"youtube","level":"tamed","toggles":{"hideShorts":true},"paths":[{"path":"/shorts","level":"restricted"}]},{"recipeId":"site:example.com","level":"redirected"}]"#;
        let sites: Vec<SiteRule> = serde_json::from_str(saved).unwrap();
        assert_eq!(sites[0].level, Tamed);
        assert_eq!(sites[1].paths, None);
        assert_eq!(serde_json::to_string(&sites).unwrap(), saved);
    }

    #[test]
    fn tidying_drops_what_cant_be_a_site_or_a_page() {
        let sites = tidy(vec![
            site("reddit", Tamed, &[("/r/all", Restricted), ("no-slash", Restricted)]),
            site("site:example.com", Restricted, &[]),
            site("site:192.168.1.1", Restricted, &[]),
            site("site:localhost", Restricted, &[]),
            site("Not a site", Restricted, &[]),
            site("", Restricted, &[]),
        ]);
        let ids: Vec<&str> = sites.iter().map(|s| s.recipe_id.as_str()).collect();
        assert_eq!(ids, ["reddit", "site:example.com"]);
        assert_eq!(sites[0].paths.as_ref().unwrap().len(), 1);
    }

    fn hiding(id: &str, selectors: &[&str]) -> SiteSettings {
        let hidden = selectors
            .iter()
            .map(|&selector| HiddenPart {
                selector: selector.into(),
                label: "A part".into(),
            })
            .collect();
        SiteSettings {
            recipe_id: id.into(),
            hidden: Some(hidden),
            all_the_time: None,
            toggles: None,
        }
    }

    #[test]
    fn reads_site_settings_and_writes_them_back_the_same() {
        let saved = r#"[{"recipeId":"reddit","hidden":[{"selector":"aside.trending","label":"Trending now"}],"allTheTime":true,"toggles":{"grayscale":true}}]"#;
        let settings: Vec<SiteSettings> = serde_json::from_str(saved).unwrap();
        assert_eq!(settings[0].all_the_time, Some(true));
        assert_eq!(serde_json::to_string(&settings).unwrap(), saved);
    }

    #[test]
    fn tidying_drops_hidden_parts_that_could_break_out_of_the_stylesheet() {
        let mut long = hiding(
            "site:a.com",
            &["aside", "a{}", "b;c", "@import x", "a\\62", "a /* c */", "  "],
        );
        long.hidden.as_mut().unwrap()[0].label = "x".repeat(200);
        let tidied = tidy_settings(vec![
            long,
            hiding("site:a.com", &["nav"]),
            hiding("Not a site", &["nav"]),
        ]);
        assert_eq!(tidied.len(), 1);
        let hidden = tidied[0].hidden.as_ref().unwrap();
        assert_eq!(hidden.len(), 1);
        assert_eq!(hidden[0].selector, "aside");
        assert_eq!(hidden[0].label.len(), MAX_LABEL_LENGTH);
    }

    #[test]
    fn merging_a_backup_adds_hidden_parts_and_keeps_toggles_here() {
        let mut here = vec![SiteSettings {
            all_the_time: Some(true),
            toggles: Some(BTreeMap::from([("grayscale".into(), true)])),
            ..hiding("reddit", &["aside"])
        }];
        let theirs = vec![
            SiteSettings {
                all_the_time: Some(false),
                toggles: Some(BTreeMap::from([("grayscale".into(), false)])),
                ..hiding("reddit", &["aside", "nav"])
            },
            hiding("site:a.com", &["footer"]),
        ];
        assert!(merge_settings(&mut here, theirs.clone()));
        let selectors: Vec<&str> = here[0].hidden.iter().flatten().map(|p| p.selector.as_str()).collect();
        assert_eq!(selectors, ["aside", "nav"]);
        assert_eq!(here[0].all_the_time, Some(true));
        assert!(here[0].toggles.as_ref().unwrap()["grayscale"]);
        assert_eq!(here[1].recipe_id, "site:a.com");
        assert!(!merge_settings(&mut here, theirs));
    }

    #[test]
    fn stricter_changes_and_new_sites_are_not_looser() {
        let old = vec![
            site("youtube", Tamed, &[("/shorts", Restricted)]),
            site("site:a.com", Redirected, &[]),
        ];
        assert!(!looser(list(&old), list(&old)));
        let new = vec![
            site("youtube", Redirected, &[("/SHORTS", Restricted)]),
            site("site:a.com", Restricted, &[]),
            site("site:b.com", Redirected, &[]),
        ];
        assert!(!looser(list(&old), list(&new)));
    }

    #[test]
    fn loosening_a_site_or_page_or_taking_one_off_is_looser() {
        let old = vec![
            site("youtube", Tamed, &[("/shorts", Restricted)]),
            site("site:a.com", Redirected, &[]),
        ];
        let youtube = |paths: &[(&str, SiteLevel)]| vec![site("youtube", Tamed, paths), old[1].clone()];
        assert!(looser(list(&old), list(&[old[0].clone()])));
        assert!(looser(
            list(&old),
            list(&[old[0].clone(), site("site:a.com", Allowed, &[])])
        ));
        assert!(looser(list(&old), list(&youtube(&[("/shorts", Redirected)]))));
        assert!(looser(list(&old), list(&youtube(&[]))));
        assert!(looser(
            list(&old),
            list(&youtube(&[("/shorts", Restricted), ("/shorts/abc", Tamed)]))
        ));
    }

    #[test]
    fn every_other_site_counts_for_sites_added_or_taken_off() {
        let docs = vec![site("site:docs.com", Allowed, &[])];
        // Every other site loosened.
        assert!(looser(allowlist(&docs, Restricted), allowlist(&docs, Redirected)));
        assert!(looser(allowlist(&docs, Restricted), list(&docs)));
        assert!(!looser(list(&docs), allowlist(&docs, Restricted)));
        // A site added looser than every other site, or with a looser page.
        let mut more = docs.clone();
        more.push(site("reddit", Tamed, &[]));
        assert!(looser(allowlist(&docs, Restricted), allowlist(&more, Restricted)));
        more[1] = site("reddit", Restricted, &[("/r/a", Allowed)]);
        assert!(looser(allowlist(&docs, Restricted), allowlist(&more, Restricted)));
        more[1] = site("reddit", Restricted, &[]);
        assert!(!looser(allowlist(&docs, Restricted), allowlist(&more, Restricted)));
        // Taken off, a site joins every other site.
        assert!(!looser(allowlist(&docs, Restricted), allowlist(&[], Restricted)));
        let restricted = vec![site("x", Restricted, &[])];
        assert!(looser(allowlist(&restricted, Redirected), allowlist(&[], Redirected)));
    }

    #[test]
    fn every_other_site_is_never_tamed() {
        assert_eq!(other_sites(None), Allowed);
        assert_eq!(other_sites(Some(Tamed)), Allowed);
        assert_eq!(other_sites(Some(Restricted)), Restricted);
    }
}
