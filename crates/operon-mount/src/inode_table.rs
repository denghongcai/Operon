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

    pub(crate) fn rename_subtree(
        &mut self,
        from_path: &str,
        to_path: &str,
        new_parent: fuser::INodeNo,
        new_name: String,
    ) -> anyhow::Result<()> {
        let from_path = normalize_remote_path(from_path)?;
        let to_path = normalize_remote_path(to_path)?;
        let from_prefix = if from_path == "/" {
            "/".to_string()
        } else {
            format!("{from_path}/")
        };

        let renamed: Vec<u64> = self
            .by_path
            .iter()
            .filter_map(|(entry_path, ino)| {
                (entry_path == &from_path || entry_path.starts_with(&from_prefix)).then_some(*ino)
            })
            .collect();

        for ino in &renamed {
            if let Some(entry) = self.by_ino.get(ino) {
                self.by_path.remove(&entry.path);
            }
        }

        for ino in renamed {
            if let Some(entry) = self.by_ino.get_mut(&ino) {
                let suffix = entry.path.strip_prefix(&from_path).unwrap_or_default();
                entry.path = format!("{to_path}{suffix}");
                if entry.path.is_empty() {
                    entry.path = "/".to_string();
                }
                if entry.path == to_path {
                    entry.parent = new_parent;
                    entry.name = new_name.clone();
                }
                self.by_path.insert(entry.path.clone(), ino);
            }
        }

        Ok(())
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

    #[test]
    fn rename_subtree_preserves_inode_for_kernel_dentry() {
        let root = FsStat {
            path: "/".to_string(),
            is_file: false,
            is_dir: true,
            size: 0,
            version: "root".to_string(),
        };
        let mut table = InodeTable::new(root);
        let dir = table
            .upsert(
                fuser::INodeNo::ROOT,
                "dir".to_string(),
                FsStat {
                    path: "/dir".to_string(),
                    is_file: false,
                    is_dir: true,
                    size: 0,
                    version: "dir".to_string(),
                },
            )
            .expect("dir");
        let file = table
            .upsert(
                dir.ino,
                "data.txt".to_string(),
                FsStat {
                    path: "/dir/data.txt".to_string(),
                    is_file: true,
                    is_dir: false,
                    size: 3,
                    version: "file".to_string(),
                },
            )
            .expect("file");

        table
            .rename_subtree(
                "/dir/data.txt",
                "/dir/renamed.txt",
                dir.ino,
                "renamed.txt".to_string(),
            )
            .expect("rename subtree");

        let renamed = table.get(file.ino).expect("renamed inode");
        assert_eq!(renamed.ino, file.ino);
        assert_eq!(renamed.parent, dir.ino);
        assert_eq!(renamed.name, "renamed.txt");
        assert_eq!(renamed.path, "/dir/renamed.txt");
    }

    #[test]
    fn remove_then_rename_replaces_destination_inode() {
        let root = FsStat {
            path: "/".to_string(),
            is_file: false,
            is_dir: true,
            size: 0,
            version: "root".to_string(),
        };
        let mut table = InodeTable::new(root);
        let source = table
            .upsert(
                fuser::INodeNo::ROOT,
                "source.txt".to_string(),
                FsStat {
                    path: "/source.txt".to_string(),
                    is_file: true,
                    is_dir: false,
                    size: 3,
                    version: "source".to_string(),
                },
            )
            .expect("source");
        let destination = table
            .upsert(
                fuser::INodeNo::ROOT,
                "destination.txt".to_string(),
                FsStat {
                    path: "/destination.txt".to_string(),
                    is_file: true,
                    is_dir: false,
                    size: 9,
                    version: "destination".to_string(),
                },
            )
            .expect("destination");

        table.remove_subtree("/destination.txt");
        table
            .rename_subtree(
                "/source.txt",
                "/destination.txt",
                fuser::INodeNo::ROOT,
                "destination.txt".to_string(),
            )
            .expect("rename over destination");

        assert!(table.get(destination.ino).is_none());
        let renamed = table.get(source.ino).expect("source inode renamed");
        assert_eq!(renamed.path, "/destination.txt");
        assert_eq!(renamed.name, "destination.txt");
    }
}
