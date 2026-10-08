// Update check: ask GitHub for the latest published Slab release and compare
// it with the running version.
//
// Slab promises never to call home, so this only ever runs when the user asks
// (Settings → Check for updates) or has opted in to a check at launch. The one
// request goes to GitHub's public releases API and carries no identifiers:
// no account, no machine id, no document data.

use serde::{Deserialize, Serialize};
use std::time::Duration;

const LATEST_URL: &str = "https://api.github.com/repos/Sanjays2402/slab/releases/latest";
/// Release pages must live under this prefix before the UI is allowed to open them.
const RELEASE_URL_PREFIX: &str = "https://github.com/Sanjays2402/slab/";
const MAX_NOTES_CHARS: usize = 4000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
    pub url: String,
    pub notes: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// `v3.41.1`, `3.41.1`, `3.42.0-rc.1` → `(3, 41, 1)`. Missing parts are 0;
/// anything non-numeric in the core version is rejected.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let core = s
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = match parts.next() {
        Some(p) => p.parse().ok()?,
        None => 0,
    };
    let patch = match parts.next() {
        Some(p) => p.parse().ok()?,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// True when `latest` is a strictly newer version than `current`.
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_version(current), parse_version(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

fn build_info(current: &str, rel: GithubRelease) -> Result<UpdateInfo, String> {
    if rel.draft || rel.prerelease {
        return Err("latest release is not a stable release".into());
    }
    if parse_version(&rel.tag_name).is_none() {
        return Err(format!("unrecognised release tag {:?}", rel.tag_name));
    }
    if !rel.html_url.starts_with(RELEASE_URL_PREFIX) {
        return Err("release URL is not a Slab GitHub URL".into());
    }
    let notes: String = rel
        .body
        .unwrap_or_default()
        .chars()
        .take(MAX_NOTES_CHARS)
        .collect();
    Ok(UpdateInfo {
        current: current.to_string(),
        update_available: is_newer(current, &rel.tag_name),
        latest: rel.tag_name.trim_start_matches(['v', 'V']).to_string(),
        url: rel.html_url,
        notes,
    })
}

/// Fetch the latest release and compare it with `current`.
pub async fn check(current: &str) -> Result<UpdateInfo, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(format!("slab/{current}"))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(LATEST_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Couldn't reach GitHub: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub returned {}", resp.status()));
    }
    let rel: GithubRelease = resp
        .json()
        .await
        .map_err(|e| format!("Unexpected response from GitHub: {e}"))?;
    build_info(current, rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(tag: &str, url: &str) -> GithubRelease {
        GithubRelease {
            tag_name: tag.into(),
            html_url: url.into(),
            body: Some("notes".into()),
            draft: false,
            prerelease: false,
        }
    }

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("v3.41.1"), Some((3, 41, 1)));
        assert_eq!(parse_version("3.41.1"), Some((3, 41, 1)));
        assert_eq!(parse_version("3.42"), Some((3, 42, 0)));
        assert_eq!(parse_version("3.42.0-rc.1"), Some((3, 42, 0)));
        assert_eq!(parse_version("3.42.0+build5"), Some((3, 42, 0)));
        assert_eq!(parse_version("latest"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn compares_numerically_not_lexically() {
        assert!(is_newer("3.9.0", "3.10.0"));
        assert!(is_newer("3.41.0", "v3.41.1"));
        assert!(is_newer("3.41.9", "4.0.0"));
        assert!(!is_newer("3.41.1", "3.41.1"));
        assert!(!is_newer("3.41.1", "3.41.0"));
        assert!(!is_newer("3.41.1", "garbage"));
    }

    #[test]
    fn builds_info_for_newer_release() {
        let info = build_info(
            "3.41.0",
            rel(
                "v3.41.1",
                "https://github.com/Sanjays2402/slab/releases/tag/v3.41.1",
            ),
        )
        .unwrap();
        assert!(info.update_available);
        assert_eq!(info.latest, "3.41.1");
        assert_eq!(info.current, "3.41.0");
    }

    #[test]
    fn up_to_date_when_equal() {
        let info = build_info(
            "3.41.1",
            rel(
                "v3.41.1",
                "https://github.com/Sanjays2402/slab/releases/tag/v3.41.1",
            ),
        )
        .unwrap();
        assert!(!info.update_available);
    }

    #[test]
    fn rejects_foreign_urls_and_unstable_releases() {
        assert!(build_info("3.41.0", rel("v3.41.1", "https://evil.example/x")).is_err());
        assert!(build_info(
            "3.41.0",
            rel("v3.41.1", "https://github.com/Sanjays2402/slab-evil/x")
        )
        .is_err());
        let mut pre = rel(
            "v3.42.0",
            "https://github.com/Sanjays2402/slab/releases/tag/v3.42.0",
        );
        pre.prerelease = true;
        assert!(build_info("3.41.0", pre).is_err());
        assert!(build_info(
            "3.41.0",
            rel(
                "nightly",
                "https://github.com/Sanjays2402/slab/releases/tag/nightly"
            )
        )
        .is_err());
    }

    #[test]
    fn truncates_long_notes() {
        let mut r = rel(
            "v3.42.0",
            "https://github.com/Sanjays2402/slab/releases/tag/v3.42.0",
        );
        r.body = Some("x".repeat(MAX_NOTES_CHARS + 500));
        assert_eq!(
            build_info("3.41.0", r).unwrap().notes.chars().count(),
            MAX_NOTES_CHARS
        );
    }
}
