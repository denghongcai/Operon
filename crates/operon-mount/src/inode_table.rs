use std::collections::HashMap;

use operon_core::FsStat;

use crate::path::normalize_remote_path;

#[derive(Debug, Clone)]
pub(crate) struct InodeEntry {
    pub(crate) ino: fuser::INodeNo,
    pub(crate) parent: fuser::INodeNo,
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) is_dir: bool,
    pub(crate) size: u64,
}

#[derive(Debug)]
pub(crate) struct InodeTable {
    next: u64,
    by_ino: HashMap<u64, InodeEntry>,
    by_path: HashMap<String, u64>,
}

impl InodeTable {
    pub(crate) fn new(root: FsStat) -> Self {
        let root_entry = InodeEntry {
            ino: fuser::INodeNo::ROOT,
            parent: fuser::INodeNo::ROOT,
            name: ".".to_string(),
            path: root.path.clone(),
            is_dir: true,
            size: root.size,
        };
        let mut by_ino = HashMap::new();
        let mut by_path = HashMap::new();
        by_path.insert(root.path, u64::from(fuser::INodeNo::ROOT));
        by_ino.insert(u64::from(fuser::INodeNo::ROOT), root_entry);
        Self {
            next: u64::from(fuser::INodeNo::ROOT) + 1,
            by_ino,
            by_path,
        }
    }

    pub(crate) fn get(&self, ino: fuser::INodeNo) -> Option<InodeEntry> {
        self.by_ino.get(&u64::from(ino)).cloned()
    }

    pub(crate) fn remove_subtree(&mut self, path: &str) {
        let prefix = if path == "/" {
            "/".to_string()
        } else {
            format!("{path}/")
        };
        let removed: Vec<u64> = self
            .by_path
            .iter()
            .filter_map(|(entry_path, ino)| {
                (entry_path == path || entry_path.starts_with(&prefix)).then_some(*ino)
            })
            .collect();
        for ino in removed {
            if let Some(entry) = self.by_ino.remove(&ino) {
                self.by_path.remove(&entry.path);
            }
        }
    }

    pub(crate) fn upsert(
        &mut self,
        parent: fuser::INodeNo,
        name: String,
        stat: FsStat,
    ) -> anyhow::Result<InodeEntry> {
        let path = normalize_remote_path(&stat.path)?;
        let ino = if let Some(ino) = self.by_path.get(&path) {
            fuser::INodeNo(*ino)
        } else {
            let ino = fuser::INodeNo(self.next);
            self.next += 1;
            self.by_path.insert(path.clone(), u64::from(ino));
            ino
        };
        let entry = InodeEntry {
            ino,
            parent,
            name,
            path: path.clone(),
            is_dir: stat.is_dir,
            size: stat.size,
        };
        self.by_ino.insert(u64::from(ino), entry.clone());
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inode_table_reuses_paths() {
        let root = FsStat {
            path: "/".to_string(),
            is_file: false,
            is_dir: true,
            size: 0,
            version: "root".to_string(),
        };
        let mut table = InodeTable::new(root);

        let first = table
            .upsert(
                fuser::INodeNo::ROOT,
                "file.txt".to_string(),
                FsStat {
                    path: "/file.txt".to_string(),
                    is_file: true,
                    is_dir: false,
                    size: 3,
                    version: "v1".to_string(),
                },
            )
            .expect("first");
        let second = table
            .upsert(
                fuser::INodeNo::ROOT,
                "file.txt".to_string(),
                FsStat {
                    path: "/file.txt".to_string(),
                    is_file: true,
                    is_dir: false,
                    size: 5,
                    version: "v2".to_string(),
                },
            )
            .expect("second");

        assert_eq!(first.ino, second.ino);
        assert_eq!(table.get(first.ino).expect("entry").size, 5);
    }
}
