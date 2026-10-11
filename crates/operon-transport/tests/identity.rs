#![cfg(feature = "iroh")]

use operon_transport::identity::{load_identity, load_or_create_identity};
use std::{
    fs,
    sync::{Arc, Barrier},
};

#[test]
fn private_identity_survives_restart_without_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.key");
    let first = load_or_create_identity(&path).unwrap().public();
    assert_eq!(fs::metadata(&path).unwrap().len(), 32);
    operon_config::validate_private_file_permissions(&path).unwrap();
    for _ in 0..8 {
        assert_eq!(load_or_create_identity(&path).unwrap().public(), first);
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn concurrent_initializers_share_one_atomically_published_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = Arc::new(dir.path().join("identity.key"));
    let barrier = Arc::new(Barrier::new(16));
    let threads: Vec<_> = (0..16)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                load_or_create_identity(&path).unwrap().public()
            })
        })
        .collect();
    let ids: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert!(ids.iter().all(|id| *id == ids[0]));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    operon_config::validate_private_file_permissions(&path).unwrap();
}

#[test]
fn malformed_identity_is_not_overwritten_or_disclosed_in_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.key");
    let mut file = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
    operon_config::protect_new_private_file(file.path()).unwrap();
    use std::io::Write;
    file.write_all(b"malformed-private-key-sentinel").unwrap();
    file.persist(&path).unwrap();
    let before = fs::read(&path).unwrap();
    let error = load_or_create_identity(&path).unwrap_err();
    assert!(!error.to_string().contains("sentinel"));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn deliberate_rotation_changes_public_identity_only_after_file_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.key");
    let first = load_or_create_identity(&path).unwrap().public();
    fs::rename(&path, dir.path().join("identity.previous")).unwrap();
    let second = load_or_create_identity(&path).unwrap().public();
    assert_ne!(first, second);
    assert_eq!(
        load_identity(&dir.path().join("identity.previous"))
            .unwrap()
            .public(),
        first
    );
}

#[cfg(unix)]
#[test]
fn broad_identity_permissions_are_rejected_without_automatic_repair() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.key");
    let first = load_or_create_identity(&path).unwrap().public();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(load_or_create_identity(&path).is_err());
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(load_identity(&path).unwrap().public(), first);
}

#[cfg(unix)]
#[test]
fn symlink_identity_is_rejected_without_modifying_target() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("existing.key");
    let id = load_or_create_identity(&target).unwrap().public();
    let path = dir.path().join("identity.key");
    symlink(&target, &path).unwrap();
    assert!(load_or_create_identity(&path).is_err());
    assert_eq!(load_identity(&target).unwrap().public(), id);
    assert!(fs::symlink_metadata(&path)
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn protection_requires_empty_new_file_and_verifies_held_handle() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let mut staged = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
    operon_config::protect_new_private_file(staged.path()).unwrap();
    operon_config::validate_private_file_handle_permissions(staged.as_file()).unwrap();
    staged.write_all(b"private-sentinel").unwrap();
    assert!(operon_config::protect_new_private_file(staged.path()).is_err());
    operon_config::validate_private_file_handle_permissions(staged.as_file()).unwrap();
}
