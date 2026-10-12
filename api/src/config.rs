//! Environment-driven configuration, read once at startup.

use std::env;

/// Clearly-not-a-real-address placeholder. The Stream contract this API indexes
/// lives in the sister `stellar-stream-hive-contract` repo and has not been
/// deployed to a network yet as of writing this indexer. Once it is deployed, set
/// the real `C...` contract address via the `STREAM_CONTRACT_ID` environment
/// variable -- do not hardcode a real address here. See
/// docs/app/docs/getting-started/page.mdx ("Deployed contract addresses") for
/// where the real addresses will be published once available.
pub const STREAM_CONTRACT_ID_PLACEHOLDER: &str = "CONTRACT_ID_NOT_YET_DEPLOYED_SET_STREAM_CONTRACT_ID_ENV_VAR";

pub const DEFAULT_HORIZON_URL: &str = "https://horizon-testnet.stellar.org";
pub const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 10;

#[derive(Debug, Clone)]
pub struct Config {
    /// Base URL of the Horizon instance to poll. Defaults to public testnet.
    pub horizon_url: String,
    /// The Stream contract's `C...` strkey address. See `STREAM_CONTRACT_ID_PLACEHOLDER`
    /// for why this defaults to an obvious placeholder rather than a real-looking value.
    pub stream_contract_id: String,
    /// Socket address the HTTP server binds to.
    pub bind_addr: String,
    /// How often the indexer polls Horizon for new activity.
    pub poll_interval_secs: u64,
    /// Browser origins allowed to call the API (`CORS_ALLOWED_ORIGINS`, comma
    /// separated). Empty means any origin, which is safe for this read-only,
    /// credential-free API; see [`crate::cors`].
    pub cors_allowed_origins: Vec<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            horizon_url: env::var("HORIZON_URL").unwrap_or_else(|_| DEFAULT_HORIZON_URL.to_string()),
            stream_contract_id: env::var("STREAM_CONTRACT_ID")
                .unwrap_or_else(|_| STREAM_CONTRACT_ID_PLACEHOLDER.to_string()),
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string()),
            poll_interval_secs: env::var("POLL_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_POLL_INTERVAL_SECS),
            cors_allowed_origins: parse_origins(&env::var("CORS_ALLOWED_ORIGINS").unwrap_or_default()),
        }
    }

    /// Whether `stream_contract_id` looks like a real `C...` strkey contract address
    /// (as opposed to the startup placeholder). The indexer uses this to decide
    /// whether it can meaningfully filter/decode Soroban contract events yet.
    pub fn has_real_contract_id(&self) -> bool {
        self.stream_contract_id != STREAM_CONTRACT_ID_PLACEHOLDER
            && stellar_strkey::Contract::from_string(&self.stream_contract_id).is_ok()
    }
}

/// Parses a comma-separated origin list. Whitespace and trailing slashes are
/// trimmed (browsers send origins without a trailing slash), empty entries are
/// dropped, and a lone `*` means "any origin" -- the same as leaving it unset.
pub fn parse_origins(raw: &str) -> Vec<String> {
    let origins: Vec<String> = raw
        .split(',')
        .map(|o| o.trim().trim_end_matches('/').to_string())
        .filter(|o| !o.is_empty())
        .collect();
    if origins.iter().any(|o| o == "*") {
        Vec::new()
    } else {
        origins
    }
}

#[cfg(test)]
mod tests {
    use super::parse_origins;

    #[test]
    fn unset_or_blank_means_any_origin() {
        assert!(parse_origins("").is_empty());
        assert!(parse_origins("  , ,").is_empty());
    }

    #[test]
    fn star_means_any_origin_even_among_others() {
        assert!(parse_origins("*").is_empty());
        assert!(parse_origins("https://a.example, *").is_empty());
    }

    #[test]
    fn trims_whitespace_and_trailing_slashes() {
        assert_eq!(
            parse_origins(" https://a.example/ ,https://b.example:3000 ,, "),
            vec!["https://a.example", "https://b.example:3000"]
        );
    }
}
