//! Where DZBuild lives and who the POS is to it (spec §4.7). Public values only: the installer
//! ships them in `src-tauri/dzbuild.toml`. There is no field for a secret, and unknown keys refuse
//! the whole file, so a `client_secret` line can never be configured.

use crate::error::AppError;
use serde::Deserialize;
use url::Url;

/// The first-party public client (PKCE, `token_endpoint_auth_method: none`).
pub const PUBLIC_CLIENT_ID: &str = "dzbuild-pos-windows";
/// Chooses the store of a multi-store account on every API call (Backend contract).
pub const DEFAULT_STORE_HEADER: &str = "X-DZ-Store";
/// Every scope of the Backend Brief the POS uses; explained in plain words in Settings › DZBuild.
pub const DEFAULT_SCOPES: &[&str] = &[
    "openid",
    "profile",
    "offline_access",
    "store:read",
    "products:read",
    "products:write",
    "inventory:read",
    "inventory:write",
    "orders:read",
    "orders:write",
    "customers:read",
    "pos:sales:read",
    "pos:sales:write",
    "locations:read",
    "locations:write",
    "devices:self",
    "events:read",
];
pub const UTM: &str = "utm_source=dzbuild-pos&utm_medium=app&utm_campaign=link";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudConfig {
    pub issuer: String,
    pub api_base: String,
    pub client_id: String,
    pub signup_url: String,
    pub devices_url: String,
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
    #[serde(default = "default_store_header")]
    pub store_header: String,
}

fn default_scopes() -> Vec<String> {
    DEFAULT_SCOPES.iter().map(|s| (*s).to_owned()).collect()
}

fn default_store_header() -> String {
    DEFAULT_STORE_HEADER.to_owned()
}

fn invalid() -> AppError {
    AppError::sync("cloud_config_invalid")
}

/// `https`, or `http` on the loopback address only (local mocks); no user info, no fragment.
pub fn check_url(value: &str) -> Result<Url, AppError> {
    let url = Url::parse(value).map_err(|_| invalid())?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    let scheme_ok = url.scheme() == "https" || (url.scheme() == "http" && loopback);
    if scheme_ok
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
    {
        Ok(url)
    } else {
        Err(invalid())
    }
}

fn looks_secret(value: &str) -> bool {
    ["dzas_", "dzpk_live_", "dzpk_test_", "secret"]
        .iter()
        .any(|p| value.contains(p))
}

impl CloudConfig {
    /// A configuration pointing at a local mock (`http://127.0.0.1:<port>`): wiremock tests only.
    pub fn loopback(base: &str) -> CloudConfig {
        let base = base.trim_end_matches('/');
        CloudConfig {
            issuer: base.to_owned(),
            api_base: format!("{base}/v1"),
            client_id: PUBLIC_CLIENT_ID.to_owned(),
            signup_url: format!("{base}/register"),
            devices_url: format!("{base}/devices"),
            scopes: default_scopes(),
            store_header: default_store_header(),
        }
    }

    pub fn validate(&self) -> Result<(), AppError> {
        for value in [
            &self.issuer,
            &self.api_base,
            &self.signup_url,
            &self.devices_url,
        ] {
            check_url(value)?;
        }
        let client_ok = !self.client_id.trim().is_empty()
            && self.client_id.len() <= 64
            && !looks_secret(&self.client_id);
        let scopes_ok = self
            .scopes
            .iter()
            .all(|s| !s.is_empty() && !s.contains(char::is_whitespace))
            && self.scopes.iter().any(|s| s == "offline_access");
        let header_ok = !self.store_header.is_empty()
            && self
                .store_header
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if client_ok && scopes_ok && header_ok {
            Ok(())
        } else {
            Err(invalid())
        }
    }

    /// `api_base` + `path` (`path` starts with `/`).
    pub fn api_url(&self, path: &str) -> Result<Url, AppError> {
        check_url(&format!("{}{path}", self.api_base.trim_end_matches('/')))
    }

    /// RFC 8414 metadata of the issuer.
    pub fn well_known(&self) -> Result<Url, AppError> {
        check_url(&format!(
            "{}/.well-known/oauth-authorization-server",
            self.issuer.trim_end_matches('/')
        ))
    }

    pub fn scope(&self) -> String {
        self.scopes.join(" ")
    }

    /// "Create your online store" (DZB-019), with the growth-loop UTM of research 06 §5.7.
    pub fn signup_link(&self) -> String {
        let sep = if self.signup_url.contains('?') {
            '&'
        } else {
            '?'
        };
        format!("{}{sep}{UTM}", self.signup_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bundled() -> serde_json::Value {
        json!({
            "issuer": "https://dzbuild.com",
            "api_base": "https://api.dzbuild.app/v1",
            "client_id": "dzbuild-pos-windows",
            "signup_url": "https://dzbuild.com/register",
            "devices_url": "https://dzbuild.com/dashboard/connected-devices"
        })
    }

    #[test]
    fn the_bundled_values_are_valid_and_public() {
        let config: CloudConfig = serde_json::from_value(bundled()).unwrap();
        config.validate().unwrap();
        assert_eq!(config.client_id, PUBLIC_CLIENT_ID);
        assert!(config.scopes.iter().any(|s| s == "offline_access"));
        assert_eq!(config.store_header, "X-DZ-Store");
        assert_eq!(
            config.well_known().unwrap().as_str(),
            "https://dzbuild.com/.well-known/oauth-authorization-server"
        );
        assert_eq!(
            config.api_url("/pos/sales").unwrap().as_str(),
            "https://api.dzbuild.app/v1/pos/sales"
        );
        assert_eq!(
            config.signup_link(),
            "https://dzbuild.com/register?utm_source=dzbuild-pos&utm_medium=app&utm_campaign=link"
        );
    }

    #[test]
    fn a_client_secret_in_the_config_is_refused() {
        let mut value = bundled();
        value["client_secret"] = json!("dzas_0123456789abcdef");
        assert!(
            serde_json::from_value::<CloudConfig>(value).is_err(),
            "unknown keys refuse the file"
        );
        let mut value = bundled();
        value["client_id"] = json!("dzpk_live_1.abcdef");
        let config: CloudConfig = serde_json::from_value(value).unwrap();
        assert_eq!(
            config.validate().unwrap_err().code(),
            "cloud_config_invalid"
        );
    }

    #[test]
    fn only_https_or_the_loopback_mock_is_accepted() {
        assert!(check_url("https://api.dzbuild.app/v1").is_ok());
        assert!(check_url("http://127.0.0.1:61234").is_ok());
        for bad in [
            "http://api.dzbuild.app/v1",
            "https://user:pw@dzbuild.com",
            "https://dzbuild.com/#frag",
            "ftp://dzbuild.com",
            "not a url",
        ] {
            assert_eq!(
                check_url(bad).unwrap_err().code(),
                "cloud_config_invalid",
                "{bad}"
            );
        }
        let mut config = CloudConfig::loopback("http://127.0.0.1:61234");
        config.validate().unwrap();
        config.scopes.retain(|s| s != "offline_access");
        assert_eq!(
            config.validate().unwrap_err().code(),
            "cloud_config_invalid",
            "refresh tokens are required"
        );
    }
}
