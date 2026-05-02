#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ServicePolicy {
    pub services: Vec<ServiceDefinition>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceDefinition {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub protocol: ServiceProtocol,
    pub description: String,
    #[serde(default)]
    pub permissions: ServicePermissions,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServicePermissions {
    pub check: bool,
    pub forward: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceProtocol {
    Tcp,
    Udp,
}

impl Default for ServicePermissions {
    fn default() -> Self {
        Self {
            check: true,
            forward: true,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceList {
    pub services: Vec<ServiceDefinition>,
    #[serde(default)]
    pub next_page_token: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceCheck {
    pub id: String,
    pub ok: bool,
    pub latency_ms: u128,
    pub reason: Option<String>,
}
