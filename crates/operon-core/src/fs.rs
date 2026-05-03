#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsStat {
    pub path: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsList {
    pub path: String,
    pub entries: Vec<FsEntry>,
    pub next_page_token: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsRead {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsReadRangeRequest {
    pub path: String,
    pub offset: u64,
    pub size: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsReadRange {
    pub path: String,
    pub offset: u64,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsWriteRequest {
    pub path: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precondition: Option<FsPrecondition>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsWrite {
    pub path: String,
    pub bytes_written: u64,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FsPrecondition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<String>,
    #[serde(default)]
    pub require_absent: bool,
}
