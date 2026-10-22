//! Update check: asks GitHub for the latest release of dbambus/VisiCutR
//! and compares it with the running version. Both apps check at most every
//! 14 days unless the user turned it off in the settings.
use crate::{i18n::tr, tr};
use serde::Serialize;
use std::time::Duration;

/// Latest published release (drafts and pre-releases are not listed here).
pub const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/dbambus/VisiCutR/releases/latest";
/// Page with all releases, for the link when no single release is known.
pub const REPOSITORY: &str = "https://github.com/dbambus/VisiCutR";
pub const RELEASES_PAGE: &str = "https://github.com/dbambus/VisiCutR/releases";
/// Pause between two automatic checks.
pub const INTERVAL_SECS: u64 = 14 * 24 * 60 * 60;

/// A release newer than the running version.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub notes: String,
}

/// Version of this build, e.g. `0.19.0`.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Comparable form of `v1.2.3` or `1.2.3-rc.1`: the numbers, then whether
/// it is a final release (it follows its pre-releases), then the number at the
/// end of the pre-release suffix (`rc.2` after `rc.1`).
fn parse(version: &str) -> Option<([u64; 3], bool, u64)> {
    let version = version.trim().trim_start_matches(['v', 'V']);
    let version = version.split('+').next()?;
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let numbers = [
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
        parts.next().flatten().unwrap_or(0),
    ];
    let pre_number = pre.map_or(0, |pre| {
        let digits: String = pre
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        digits.parse().unwrap_or(0)
    });
    Some((numbers, pre.is_none(), pre_number))
}

/// Whether `latest` is a higher version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse(latest), parse(current)), (Some(l), Some(c)) if l > c)
}

/// Whether an automatic check is due: never checked, 14 days passed, or the
/// clock went backwards.
pub fn is_due(last_check_secs: u64, now_secs: u64) -> bool {
    last_check_secs == 0
        || now_secs < last_check_secs
        || now_secs - last_check_secs >= INTERVAL_SECS
}

/// Seconds since 1970 (for the time of the last check).
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Reads GitHub's answer for the latest release; `None` if it is not newer.
fn newer_release(json: &str, current: &str) -> Result<Option<Release>, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tag = value["tag_name"]
        .as_str()
        .ok_or(tr("Antwort von GitHub ohne Versionsnummer"))?;
    if value["draft"].as_bool() == Some(true)
        || value["prerelease"].as_bool() == Some(true)
        || !is_newer(tag, current)
    {
        return Ok(None);
    }
    Ok(Some(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        url: value["html_url"]
            .as_str()
            .unwrap_or(RELEASES_PAGE)
            .to_string(),
        notes: value["body"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string(),
    }))
}

/// Asks GitHub for the latest release of the fork.
pub fn check() -> Result<Option<Release>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let body = agent
        .get(LATEST_RELEASE_API)
        // GitHub's API requires a user agent.
        .header("User-Agent", &format!("VisiCutR/{}", current_version()))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| tr!("Update-Prüfung fehlgeschlagen: {error}", error = e))?
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .map_err(|e| tr!("Update-Prüfung fehlgeschlagen: {error}", error = e))?;
    newer_release(&body, current_version())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("v0.20.0", "0.19.0"));
        assert!(is_newer("v0.19.1", "0.19.0"));
        assert!(is_newer("1.0", "0.19.9"));
        assert!(!is_newer("v0.19.0", "0.19.0"));
        assert!(!is_newer("v0.9.0", "0.19.0"));
        assert!(!is_newer("nightly", "0.19.0"));
        // Vorabversionen liegen vor der fertigen Version.
        assert!(is_newer("v1.0.0", "1.0.0-rc.1"));
        assert!(is_newer("v1.0.0-rc.2", "1.0.0-rc.1"));
        assert!(is_newer("v1.0.0-rc.1", "0.20.1"));
        assert!(!is_newer("v1.0.0-rc.1", "1.0.0"));
        assert!(!is_newer("v1.0.0-rc.1", "1.0.0-rc.1"));
    }

    #[test]
    fn checks_every_fourteen_days() {
        let day = 24 * 60 * 60;
        assert!(is_due(0, 100));
        assert!(!is_due(1000, 1000 + 13 * day));
        assert!(is_due(1000, 1000 + 14 * day));
        assert!(is_due(1000, 500));
    }

    #[test]
    fn reads_github_release_answers() {
        let answer = r#"{"tag_name":"v0.20.0","html_url":"https://github.com/dbambus/VisiCutR/releases/tag/v0.20.0","body":"- Neu\n","draft":false,"prerelease":false}"#;
        let release = newer_release(answer, "0.19.0").unwrap().unwrap();
        assert_eq!(release.version, "0.20.0");
        assert!(release.url.ends_with("v0.20.0"));
        assert_eq!(release.notes, "- Neu");
        assert_eq!(newer_release(answer, "0.20.0").unwrap(), None);
        let pre = answer.replace(r#""prerelease":false"#, r#""prerelease":true"#);
        assert_eq!(newer_release(&pre, "0.19.0").unwrap(), None);
        assert!(newer_release("{}", "0.19.0").is_err());
    }
}
