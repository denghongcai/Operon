//! Configuration data only. No networking framework or protocol dependency.

use std::{net::SocketAddr, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct IrohEndpointConfig {
    pub identity_file: PathBuf,
    pub bind_addrs: Vec<SocketAddr>,
    pub relay_urls: Vec<String>,
    pub allow_insecure_relays: bool,
    pub bind_timeout_secs: u64,
    pub readiness_timeout_secs: u64,
    pub idle_timeout_secs: u64,
    pub keepalive_interval_secs: u64,
    pub path_idle_timeout_secs: u64,
    pub path_keepalive_interval_secs: u64,
}

impl Default for IrohEndpointConfig {
    fn default() -> Self {
        Self {
            // Selection is explicit: server and client must not accidentally
            // share a key when both live in one config directory.
            identity_file: PathBuf::new(),
            bind_addrs: vec![SocketAddr::from(([0, 0, 0, 0], 0))],
            relay_urls: vec![],
            allow_insecure_relays: false,
            bind_timeout_secs: 10,
            readiness_timeout_secs: 30,
            idle_timeout_secs: 120,
            keepalive_interval_secs: 5,
            path_idle_timeout_secs: 60,
            path_keepalive_interval_secs: 5,
        }
    }
}

impl IrohEndpointConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.identity_file.as_os_str().is_empty() {
            return Err("iroh identity_file must not be empty".into());
        }
        if self.bind_addrs.is_empty() && self.relay_urls.is_empty() {
            return Err("iroh needs explicit bind_addrs or relay_urls".into());
        }
        if self.bind_addrs.len() > 16 || self.relay_urls.len() > 16 {
            return Err("iroh supports at most 16 bind addresses and relays per endpoint".into());
        }
        validate_relays(&self.relay_urls, self.allow_insecure_relays)?;
        for (name, seconds, allow_zero) in [
            ("bind_timeout_secs", self.bind_timeout_secs, true),
            ("readiness_timeout_secs", self.readiness_timeout_secs, true),
            ("idle_timeout_secs", self.idle_timeout_secs, true),
            (
                "keepalive_interval_secs",
                self.keepalive_interval_secs,
                false,
            ),
            ("path_idle_timeout_secs", self.path_idle_timeout_secs, false),
            (
                "path_keepalive_interval_secs",
                self.path_keepalive_interval_secs,
                false,
            ),
        ] {
            if seconds > 604800 || (!allow_zero && seconds == 0) {
                return Err(format!(
                    "iroh {name} must be {}..=604800 seconds",
                    if allow_zero { 0 } else { 1 }
                ));
            }
        }
        if self.idle_timeout_secs != 0 && self.keepalive_interval_secs >= self.idle_timeout_secs {
            return Err("iroh keepalive interval must be less than idle timeout".into());
        }
        if self.path_keepalive_interval_secs >= self.path_idle_timeout_secs {
            return Err("iroh path keepalive interval must be less than path idle timeout".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct IrohConnectionConfig {
    #[serde(flatten)]
    pub local: IrohEndpointConfig,
    pub peer_addrs: Vec<SocketAddr>,
    pub peer_relay_urls: Vec<String>,
}

impl IrohConnectionConfig {
    pub fn validate(&self) -> Result<(), String> {
        self.local.validate()?;
        if self.peer_addrs.is_empty() && self.peer_relay_urls.is_empty() {
            return Err("iroh target needs explicit peer_addrs or peer_relay_urls; no implicit public lookup".into());
        }
        if self.peer_addrs.len() > 16 || self.peer_relay_urls.len() > 16 {
            return Err("iroh supports at most 16 peer IP addresses and relay URLs".into());
        }
        validate_relays(&self.peer_relay_urls, self.local.allow_insecure_relays)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct IrohListenerConfig {
    #[serde(flatten)]
    pub endpoint: IrohEndpointConfig,
    pub max_connections: usize,
    pub handshake_timeout_secs: u64,
}

impl Default for IrohListenerConfig {
    fn default() -> Self {
        Self {
            endpoint: Default::default(),
            max_connections: 128,
            handshake_timeout_secs: 10,
        }
    }
}

impl IrohListenerConfig {
    pub fn validate(&self) -> Result<(), String> {
        self.endpoint.validate()?;
        if !(1..=4096).contains(&self.max_connections) {
            return Err("iroh max_connections must be 1..=4096".into());
        }
        if self.handshake_timeout_secs > 604800 {
            return Err("iroh handshake_timeout_secs must be 0..=604800".into());
        }
        Ok(())
    }
}

fn validate_relays(urls: &[String], allow_insecure: bool) -> Result<(), String> {
    for url in urls {
        let supported =
            url.starts_with("https://") || (allow_insecure && url.starts_with("http://"));
        if !supported || url.contains('@') || url.chars().any(char::is_whitespace) {
            return Err("iroh relay URLs require HTTPS without embedded credentials; HTTP requires explicit allow_insecure_relays".into());
        }
    }
    Ok(())
}
