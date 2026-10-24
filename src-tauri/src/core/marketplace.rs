//! Marketplace search against skills.sh — the same HTTP endpoint the CLI's
//! `find` command uses (`https://skills.sh/api/search?q=<query>&limit=20`).
//!
//! Search only; installation always goes through the CLI. The endpoint is an
//! external interface, not a permanent contract: parse tolerantly and fail
//! with a clear error instead of guessing shapes.

use crate::core::error::AppError;
use serde::Deserialize;
use std::time::Duration;

const SEARCH_URL: &str = "https://skills.sh/api/search";
const SEARCH_LIMIT: u32 = 20;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// One search result, normalized from the API's `{skills: [...]}` payload.
/// `slug` is the id used by skills.sh (`owner/repo@skill`), `source` is the
/// installable package reference (`owner/repo`).
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct MarketSkill {
    pub name: String,
    pub slug: String,
    pub source: String,
    pub installs: u64,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    skills: Vec<SearchItem>,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "installs")]
    installs: Option<u64>,
    #[serde(default)]
    source: String,
}

/// Search skills.sh. Empty query is rejected (the endpoint would return
/// nothing useful); results are sorted by installs, descending, like the CLI.
pub async fn search(query: &str, owner: Option<&str>) -> Result<Vec<MarketSkill>, AppError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::invalid_input("search query is empty"));
    }
    if let Some(owner) = owner {
        if !is_valid_owner(owner) {
            return Err(AppError::invalid_input(
                "owner must be a valid GitHub owner",
            ));
        }
    }

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("skills-manager/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(AppError::network)?;

    let mut url = reqwest::Url::parse(SEARCH_URL).map_err(|e| AppError::internal(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("limit", &SEARCH_LIMIT.to_string());
    if let Some(owner) = owner {
        url.query_pairs_mut().append_pair("owner", owner.to_lowercase().trim());
    }

    let response = client
        .get(url)
        .send()
        .await
        .map_err(AppError::network)?;
    if !response.status().is_success() {
        return Err(AppError::network(format!(
            "skills.sh search failed with HTTP {}",
            response.status()
        )));
    }
    let parsed: SearchResponse = response.json().await.map_err(AppError::network)?;
    let mut results: Vec<MarketSkill> = parsed
        .skills
        .into_iter()
        .map(|item| MarketSkill {
            name: sanitize(&item.name),
            slug: sanitize(&item.id),
            source: sanitize(&item.source),
            installs: item.installs.unwrap_or(0),
        })
        .filter(|s| !s.name.is_empty() && !s.source.is_empty())
        .collect();
    results.sort_by(|a, b| b.installs.cmp(&a.installs).then(a.name.cmp(&b.name)));
    Ok(results)
}

/// The CLI's --owner validation: `/^[a-z0-9](?:[a-z0-9-]{0,38})$/i`.
fn is_valid_owner(owner: &str) -> bool {
    let bytes = owner.as_bytes();
    if bytes.is_empty() || bytes.len() > 39 {
        return false;
    }
    let first = bytes[0];
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
}

/// Strip terminal escapes and control characters (a light version of the
/// CLI's sanitizeMetadata) so marketplace text can't smuggle ANSI noise.
fn sanitize(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_validation_matches_the_cli_regex() {
        assert!(is_valid_owner("vercel-labs"));
        assert!(is_valid_owner("a"));
        assert!(is_valid_owner("A1-b2"));
        // The CLI regex allows trailing hyphens (and 39 chars total).
        assert!(is_valid_owner("trailing-"));
        assert!(is_valid_owner(&"x".repeat(39)));
        assert!(!is_valid_owner("-leading"));
        assert!(!is_valid_owner("has space"));
        assert!(!is_valid_owner(""));
        assert!(!is_valid_owner(&"x".repeat(40)));
    }

    #[test]
    fn sanitize_strips_control_characters() {
        assert_eq!(sanitize("clean"), "clean");
        assert_eq!(sanitize("\x1b[31mred\x1b[0m"), "[31mred[0m");
        assert_eq!(sanitize("  padded  "), "padded");
    }

    #[tokio::test]
    async fn empty_query_is_invalid_input() {
        let err = search("   ", None).await.unwrap_err();
        assert!(matches!(err.kind, crate::core::error::ErrorKind::InvalidInput));
    }

    #[tokio::test]
    async fn invalid_owner_is_rejected_before_any_request() {
        let err = search("q", Some("not valid!")).await.unwrap_err();
        assert!(matches!(err.kind, crate::core::error::ErrorKind::InvalidInput));
    }

    #[test]
    fn parses_the_documented_response_shape() {
        let json = r#"{"skills":[
            {"id":"vercel-labs/agent-skills@web-design","name":"web-design","installs":120,"source":"vercel-labs/agent-skills"},
            {"id":"a/b@c","name":"c","installs":900,"source":"a/b"}
        ]}"#;
        let parsed: SearchResponse = serde_json::from_str(json).unwrap();
        let mut results: Vec<MarketSkill> = parsed
            .skills
            .into_iter()
            .map(|item| MarketSkill {
                name: sanitize(&item.name),
                slug: sanitize(&item.id),
                source: sanitize(&item.source),
                installs: item.installs.unwrap_or(0),
            })
            .collect();
        results.sort_by(|a, b| b.installs.cmp(&a.installs));
        assert_eq!(results[0].name, "c");
        assert_eq!(results[0].installs, 900);
        assert_eq!(results[1].slug, "vercel-labs/agent-skills@web-design");
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        let json = r#"{"skills":[{"name":"bare"}]}"#;
        let parsed: SearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.skills.len(), 1);
        assert_eq!(parsed.skills[0].installs, None);
    }
}
