#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsStat {
    pub path: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsList {
    pub path: String,
    pub entries: Vec<FsEntry>,
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
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsWrite {
    pub path: String,
    pub bytes_written: u64,
}
