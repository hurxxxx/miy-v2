//! Recoverable project file updates. A journal covers the whole SDK/CLI/MCP
//! update; per-file rename alone is not a multi-file transaction.
use crate::cli_error::{Category, CliError};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::File,
    path::{Path, PathBuf},
};

const JOURNAL: &str = ".miy/file-transaction.json";
const DIRECTORY: &str = ".miy/file-transactions";
const MAX_FILE: u64 = 512 * 1024 * 1024;

pub struct Change {
    pub path: String,
    pub bytes: Vec<u8>,
    pub private: bool,
    pub executable: bool,
    expected_before: Option<Option<String>>,
}

impl Change {
    pub fn new(path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            bytes: bytes.into(),
            private: false,
            executable: false,
            expected_before: None,
        }
    }
    pub fn expecting(mut self, digest: Option<String>) -> Self {
        self.expected_before = Some(digest);
        self
    }
    pub fn observe(self, root: &Path) -> Result<Self> {
        let digest = read(root, &self.path)?.as_ref().map(hash);
        Ok(self.expecting(digest))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub backup: Option<PathBuf>,
    pub changed_files: Vec<String>,
    pub recovered_previous: bool,
}

pub struct Transaction {
    root: PathBuf,
    id: String,
    recovered: bool,
    _lock: File,
}

#[derive(Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    Applying,
    Committed,
    RolledBack,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    before: Option<String>,
    after: String,
    old_mode: Option<u32>,
    old_readonly: bool,
    mode: u32,
    temporary: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u8,
    root_hash: String,
    id: String,
    phase: Phase,
    attempted: usize,
    entries: Vec<Entry>,
    created_directories: Vec<String>,
}

fn failure(code: &str, message: &str) -> anyhow::Error {
    CliError::new(Category::Environment, code, message).into()
}
fn conflict(code: &str, message: &str) -> anyhow::Error {
    CliError::new(Category::Conflict, code, message).into()
}
fn hash(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn logical(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && !path.contains(['\\', ':', '\0'])
            && path.split('/').all(|part| !matches!(part, "" | "." | "..")),
        failure(
            "managed_file_path_invalid",
            "A managed file path is outside the project."
        )
    );
    Ok(())
}

fn change_path(path: &str) -> Result<()> {
    logical(path)?;
    ensure!(
        !matches!(path, JOURNAL | ".miy/cli-setup.lock" | ".git")
            && !path.starts_with(".git/")
            && !path.starts_with(&format!("{JOURNAL}/"))
            && !path.starts_with(".miy/cli-setup.lock/")
            && path != DIRECTORY
            && !path.starts_with(&format!("{DIRECTORY}/")),
        failure(
            "managed_file_path_reserved",
            "A managed file path overlaps transaction metadata."
        )
    );
    Ok(())
}

fn safe_path(root: &Path, relative: &str, directory: bool) -> Result<PathBuf> {
    logical(relative)?;
    let mut path = root.to_path_buf();
    let parts: Vec<_> = relative.split('/').collect();
    for (index, part) in parts.iter().enumerate() {
        path.push(part);
        crate::platform::reject_reparse(&path)?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => ensure!(
                if directory || index + 1 < parts.len() {
                    metadata.is_dir()
                } else {
                    metadata.is_file()
                },
                failure(
                    "managed_file_type_conflict",
                    "A managed path has an incompatible file type."
                )
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(failure(
                    "managed_file_access_failed",
                    "A managed path cannot be inspected.",
                ));
            }
        }
    }
    Ok(path)
}

fn directory(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = safe_path(root, relative, true)?;
    let builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    let mut builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    #[cfg(not(unix))]
    let mut builder = builder;
    builder.recursive(true).create(&path)?;
    safe_path(root, relative, true)
}

fn read(root: &Path, relative: &str) -> Result<Option<Vec<u8>>> {
    let path = safe_path(root, relative, false)?;
    let metadata = match std::fs::metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(failure(
                "managed_file_read_failed",
                "A managed file cannot be read.",
            ));
        }
    };
    ensure!(
        metadata.len() <= MAX_FILE,
        failure(
            "managed_file_too_large",
            "A managed file exceeds the transaction limit."
        )
    );
    Ok(Some(std::fs::read(path)?))
}

fn probe_replace(path: &Path) -> Result<()> {
    #[cfg(windows)]
    if path.exists() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
        use windows_sys::Win32::Storage::FileSystem::{
            DELETE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        std::fs::OpenOptions::new().access_mode(GENERIC_READ | GENERIC_WRITE | DELETE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).open(path)
            .map_err(|_| conflict("managed_files_in_use", "Close the project's CLI/MCP processes and retry using the global CLI. No process was terminated."))?;
    }
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}

fn save(root: &Path, journal: &Journal) -> Result<()> {
    safe_path(root, JOURNAL, false)?;
    crate::platform::write_private(&root.join(JOURNAL), &serde_json::to_vec(journal)?)?;
    #[cfg(unix)]
    File::open(root.join(".miy"))?.sync_all()?;
    Ok(())
}

fn archive(root: &Path, journal: &Journal) -> Result<()> {
    let relative = format!("{DIRECTORY}/{}/backup", journal.id);
    directory(root, &relative)?;
    let path = safe_path(root, &format!("{relative}/manifest.json"), false)?;
    crate::platform::write_private(&path, &serde_json::to_vec_pretty(journal)?)
}

fn load(root: &Path) -> Result<Option<Journal>> {
    let Some(bytes) = read(root, JOURNAL)? else {
        return Ok(None);
    };
    ensure!(
        bytes.len() <= 8 * 1024 * 1024,
        failure(
            "file_transaction_invalid",
            "The recovery journal exceeds its size limit."
        )
    );
    let journal: Journal = serde_json::from_slice(&bytes).map_err(|_| {
        failure(
            "file_transaction_invalid",
            "The recovery journal is invalid; preserve its backup for inspection.",
        )
    })?;
    ensure!(
        journal.version == 1
            && journal.root_hash == hash(root.as_os_str().as_encoded_bytes())
            && uuid::Uuid::parse_str(&journal.id).is_ok()
            && journal.id == uuid::Uuid::parse_str(&journal.id)?.to_string()
            && journal.entries.len() <= 4096
            && journal.attempted <= journal.entries.len(),
        failure(
            "file_transaction_invalid",
            "The recovery journal does not belong to this project."
        )
    );
    let mut paths = BTreeSet::new();
    ensure!(
        match journal.phase {
            Phase::Prepared => journal.attempted == 0,
            Phase::Committed => journal.attempted == journal.entries.len(),
            Phase::Applying | Phase::RolledBack => true,
        },
        failure(
            "file_transaction_invalid",
            "The recovery journal has an inconsistent commit state."
        )
    );
    for (index, entry) in journal.entries.iter().enumerate() {
        change_path(&entry.path)?;
        ensure!(
            paths.insert(entry.path.to_lowercase())
                && valid_hash(&entry.after)
                && entry.before.as_ref().is_none_or(|value| valid_hash(value))
                && entry.mode <= 0o777
                && entry.old_mode.is_none_or(|mode| mode <= 0o777)
                && entry.temporary == temporary(&entry.path, &journal.id, index),
            failure(
                "file_transaction_invalid",
                "The recovery journal has an invalid entry."
            )
        );
    }
    for path in &journal.created_directories {
        logical(path)?;
        ensure!(
            journal
                .entries
                .iter()
                .any(|entry| entry.path.starts_with(&format!("{path}/"))),
            failure(
                "file_transaction_invalid",
                "The recovery journal has an unrelated directory."
            )
        );
    }
    Ok(Some(journal))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn data_path(id: &str, kind: &str, index: usize) -> String {
    format!("{DIRECTORY}/{id}/{kind}/{index}")
}
fn temporary(path: &str, id: &str, index: usize) -> String {
    let prefix = path
        .rsplit_once('/')
        .map_or(String::new(), |(parent, _)| format!("{parent}/"));
    format!("{prefix}.miy-replace-{id}-{index}")
}

pub fn begin(root: &Path) -> Result<Transaction> {
    crate::platform::reject_reparse(root)?;
    let root = root.canonicalize()?;
    // Share the lock used by the earlier setup implementation.
    let lock = crate::cli_guidance::operation(&root).map_err(|error| {
        if matches!(
            error.downcast_ref::<std::fs::TryLockError>(),
            Some(std::fs::TryLockError::WouldBlock)
        ) {
            conflict(
                "cli_setup_already_running_for_this_project",
                "Another setup or upgrade is running for this project.",
            )
        } else {
            failure(
                "cli_setup_lock_unavailable",
                "The project setup lock cannot be accessed.",
            )
        }
    })?;
    let recovered = recover(&root)?;
    Ok(Transaction {
        root,
        id: uuid::Uuid::new_v4().to_string(),
        recovered,
        _lock: lock,
    })
}

impl Transaction {
    /// Scratch data for dependency/component verification before project mutation.
    pub fn workspace(&self) -> Result<PathBuf> {
        directory(&self.root, &format!("{DIRECTORY}/{}/work", self.id))
    }

    fn prepare(&self, changes: Vec<Change>) -> Result<Journal> {
        ensure!(
            changes.len() <= 4096,
            failure(
                "too_many_managed_files",
                "The update contains too many managed files."
            )
        );
        let mut paths = BTreeSet::new();
        for change in &changes {
            change_path(&change.path)?;
            ensure!(
                paths.insert(change.path.to_lowercase()) && change.bytes.len() as u64 <= MAX_FILE,
                failure(
                    "managed_file_plan_invalid",
                    "The update contains duplicate paths or an oversized file."
                )
            );
            safe_path(&self.root, &change.path, false)?;
        }
        for path in &paths {
            ensure!(
                !paths
                    .iter()
                    .any(|other| path.starts_with(&format!("{other}/"))),
                failure(
                    "managed_file_plan_invalid",
                    "A planned file is also used as a directory."
                )
            );
        }
        let mut entries = vec![];
        let mut created = BTreeSet::new();
        for change in changes {
            let existing = read(&self.root, &change.path)?;
            if let Some(expected) = &change.expected_before {
                ensure!(
                    existing.as_ref().map(hash) == *expected,
                    conflict(
                        "managed_file_changed",
                        "A project file changed while the update was being prepared."
                    )
                );
            }
            let target = self.root.join(&change.path);
            let metadata = existing
                .as_ref()
                .map(|_| std::fs::metadata(&target))
                .transpose()?;
            #[cfg(unix)]
            let old_mode = {
                use std::os::unix::fs::PermissionsExt;
                metadata
                    .as_ref()
                    .map(|metadata| metadata.permissions().mode() & 0o777)
            };
            #[cfg(not(unix))]
            let old_mode = metadata.as_ref().map(|_| 0o644);
            let mode = if change.private {
                0o600
            } else if change.executable {
                0o755
            } else {
                old_mode.unwrap_or(0o644)
            };
            if existing.as_ref() == Some(&change.bytes)
                && !change.private
                && (!cfg!(unix) || old_mode == Some(mode))
            {
                continue;
            }
            // Probe every locked target before changing any project file. The
            // apply step rechecks both expected content and replaceability.
            probe_replace(&target)?;
            let index = entries.len();
            let stage = data_path(&self.id, "stage", index);
            directory(&self.root, &format!("{DIRECTORY}/{}/stage", self.id))?;
            crate::platform::write_private(&safe_path(&self.root, &stage, false)?, &change.bytes)?;
            let before = existing.as_ref().map(hash);
            if let Some(existing) = existing {
                directory(&self.root, &format!("{DIRECTORY}/{}/backup", self.id))?;
                let backup = safe_path(&self.root, &data_path(&self.id, "backup", index), false)?;
                crate::platform::write_private(&backup, &existing)?;
            }
            let mut prefix = String::new();
            let parts: Vec<_> = change.path.split('/').collect();
            for part in &parts[..parts.len() - 1] {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(part);
                if !self.root.join(&prefix).exists() {
                    created.insert(prefix.clone());
                }
            }
            entries.push(Entry {
                temporary: temporary(&change.path, &self.id, index),
                path: change.path,
                before,
                after: hash(&change.bytes),
                old_mode,
                old_readonly: metadata
                    .as_ref()
                    .is_some_and(|metadata| metadata.permissions().readonly()),
                mode,
            });
        }
        let journal = Journal {
            version: 1,
            root_hash: hash(self.root.as_os_str().as_encoded_bytes()),
            id: self.id.clone(),
            phase: Phase::Prepared,
            attempted: 0,
            entries,
            created_directories: created.into_iter().collect(),
        };
        if !journal.entries.is_empty() {
            archive(&self.root, &journal)?;
            save(&self.root, &journal)?;
        }
        Ok(journal)
    }

    fn apply_one(&self, journal: &mut Journal, index: usize) -> Result<()> {
        let entry = &journal.entries[index];
        let bytes =
            read(&self.root, &data_path(&journal.id, "stage", index))?.ok_or_else(|| {
                failure(
                    "file_transaction_stage_missing",
                    "A staged file is missing.",
                )
            })?;
        ensure!(
            hash(&bytes) == entry.after,
            failure(
                "file_transaction_stage_modified",
                "A staged file failed verification."
            )
        );
        ensure!(
            read(&self.root, &entry.path)?.as_ref().map(hash) == entry.before,
            conflict(
                "managed_file_changed",
                "A project file changed after the update was prepared."
            )
        );
        probe_replace(&self.root.join(&entry.path))?;
        journal.phase = Phase::Applying;
        journal.attempted = index + 1;
        save(&self.root, journal)?;
        publish(&self.root, &journal.entries[index], &bytes, false)
    }

    pub fn commit(self, changes: Vec<Change>) -> Result<Receipt> {
        let mut journal = self.prepare(changes)?;
        if journal.entries.is_empty() {
            return Ok(Receipt {
                backup: None,
                changed_files: vec![],
                recovered_previous: self.recovered,
            });
        }
        let result = (|| -> Result<()> {
            for index in 0..journal.entries.len() {
                self.apply_one(&mut journal, index)?;
            }
            journal.phase = Phase::Committed;
            save(&self.root, &journal)?;
            Ok(())
        })();
        if let Err(error) = result {
            if recover(&self.root).is_err() {
                return Err(conflict(
                    "file_transaction_recovery_required",
                    "The update stopped and recovery is pending. Preserve .miy/file-transactions, close this project's CLI/MCP processes, and run app setup with the global CLI.",
                ));
            }
            return Err(error);
        }
        recover(&self.root)?;
        Ok(Receipt {
            backup: Some(self.root.join(format!("{DIRECTORY}/{}/backup", self.id))),
            changed_files: journal
                .entries
                .into_iter()
                .map(|entry| entry.path)
                .collect(),
            recovered_previous: self.recovered,
        })
    }
}

fn publish(root: &Path, entry: &Entry, bytes: &[u8], restoring: bool) -> Result<()> {
    let target = safe_path(root, &entry.path, false)?;
    if let Some((parent, _)) = entry.path.rsplit_once('/') {
        let path = safe_path(root, parent, true)?;
        // Project source directories keep normal checkout permissions. Only
        // transaction scratch directories require 0700; every staged/backup or
        // sensitive config file is created privately before writing its bytes.
        std::fs::create_dir_all(path)?;
        safe_path(root, parent, true)?;
    }
    let temporary = safe_path(root, &entry.temporary, false)?;
    ensure!(
        !temporary.exists(),
        conflict(
            "file_transaction_temporary_conflict",
            "An update temporary file already exists; recover the previous update first."
        )
    );
    crate::platform::write_private(&temporary, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            &temporary,
            std::fs::Permissions::from_mode(if restoring {
                entry.old_mode.unwrap_or(0o600)
            } else {
                entry.mode
            }),
        )?;
    }
    probe_replace(&target)?;
    std::fs::rename(&temporary, &target).map_err(|_| conflict("managed_file_replace_failed", "A managed file could not be replaced. Close this project's running CLI/MCP processes and retry with the global CLI."))?;
    #[cfg(windows)]
    if restoring && entry.old_readonly {
        let mut permissions = std::fs::metadata(&target)?.permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&target, permissions)?;
    }
    #[cfg(unix)]
    File::open(target.parent().unwrap())?.sync_all()?;
    Ok(())
}

fn clear_temporary(root: &Path, entry: &Entry) -> Result<()> {
    if let Some(bytes) = read(root, &entry.temporary)? {
        let digest = hash(bytes);
        ensure!(
            digest == entry.after || entry.before.as_ref() == Some(&digest),
            conflict(
                "file_transaction_temporary_modified",
                "An update temporary file was modified; preserve it for recovery."
            )
        );
        std::fs::remove_file(root.join(&entry.temporary))?;
    }
    Ok(())
}

fn restore(root: &Path, journal: &Journal, index: usize) -> Result<()> {
    let entry = &journal.entries[index];
    clear_temporary(root, entry)?;
    let actual = read(root, &entry.path)?.as_ref().map(hash);
    if actual == entry.before {
        return Ok(());
    }
    ensure!(
        actual.as_ref() == Some(&entry.after),
        conflict(
            "file_transaction_recovery_conflict",
            "A file changed outside the interrupted update. Preserve the recovery journal and backup; resolve that file before retrying."
        )
    );
    if let Some(before) = &entry.before {
        let backup = read(root, &data_path(&journal.id, "backup", index))?.ok_or_else(|| {
            failure(
                "file_transaction_backup_missing",
                "An update backup is missing; preserve the current files and recovery journal.",
            )
        })?;
        ensure!(
            hash(&backup) == *before,
            failure(
                "file_transaction_backup_modified",
                "An update backup failed verification; current files were preserved."
            )
        );
        publish(root, entry, &backup, true)?;
    } else {
        probe_replace(&root.join(&entry.path))?;
        std::fs::remove_file(root.join(&entry.path))?;
    }
    Ok(())
}

fn recover(root: &Path) -> Result<bool> {
    let Some(mut journal) = load(root)? else {
        return Ok(false);
    };
    if !matches!(journal.phase, Phase::Committed | Phase::RolledBack) {
        let mut failed = false;
        for index in (0..journal.attempted).rev() {
            // Recover independent files even when one target is locked/edited.
            if restore(root, &journal, index).is_err() {
                failed = true;
            }
        }
        if failed {
            return Err(conflict(
                "file_transaction_recovery_required",
                "Some update files require recovery. Close this project's CLI/MCP processes and preserve its journal/backup before retrying app setup.",
            ));
        }
        for relative in journal.created_directories.iter().rev() {
            let path = safe_path(root, relative, true)?;
            match std::fs::remove_dir(path) {
                Ok(()) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                    ) => {}
                Err(error) => return Err(error.into()),
            }
        }
        journal.phase = Phase::RolledBack;
    }
    archive(root, &journal)?;
    std::fs::remove_file(root.join(JOURNAL))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn change(path: &str, text: &str) -> Change {
        Change::new(path, text.as_bytes())
    }

    #[test]
    fn commit_preserves_previous_files_and_private_backup() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a"), "old").unwrap();
        let receipt = begin(root.path())
            .unwrap()
            .commit(vec![change("a", "new"), change("dir/b", "created")])
            .unwrap();
        assert_eq!(std::fs::read(root.path().join("a")).unwrap(), b"new");
        assert_eq!(
            std::fs::read(receipt.backup.unwrap().join("0")).unwrap(),
            b"old"
        );
        assert!(!root.path().join(JOURNAL).exists());
    }

    #[test]
    fn interruption_recovers_all_applied_files_and_created_directories() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a"), "old").unwrap();
        let transaction = begin(root.path()).unwrap();
        let mut journal = transaction
            .prepare(vec![change("a", "new"), change("new/nested/b", "new")])
            .unwrap();
        transaction.apply_one(&mut journal, 0).unwrap();
        transaction.apply_one(&mut journal, 1).unwrap();
        drop(transaction); // Simulate process loss before the committed marker.
        let next = begin(root.path()).unwrap();
        assert!(next.recovered);
        assert_eq!(std::fs::read(root.path().join("a")).unwrap(), b"old");
        assert!(!root.path().join("new").exists());
        next.commit(vec![change("a", "new")]).unwrap();
        assert_eq!(std::fs::read(root.path().join("a")).unwrap(), b"new");
    }

    #[test]
    fn changed_user_file_is_preserved_and_other_files_recover() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a"), "old-a").unwrap();
        std::fs::write(root.path().join("b"), "old-b").unwrap();
        let transaction = begin(root.path()).unwrap();
        let mut journal = transaction
            .prepare(vec![change("a", "new-a"), change("b", "new-b")])
            .unwrap();
        transaction.apply_one(&mut journal, 0).unwrap();
        transaction.apply_one(&mut journal, 1).unwrap();
        std::fs::write(root.path().join("b"), "user-edit").unwrap();
        drop(transaction);
        assert!(begin(root.path()).is_err());
        assert_eq!(std::fs::read(root.path().join("a")).unwrap(), b"old-a");
        assert_eq!(std::fs::read(root.path().join("b")).unwrap(), b"user-edit");
        assert!(root.path().join(JOURNAL).exists());
        std::fs::write(root.path().join("b"), "old-b").unwrap();
        assert!(begin(root.path()).unwrap().recovered);
    }

    #[test]
    fn prepared_and_committed_journals_do_not_rollback_unapplied_or_completed_data() {
        let root = tempfile::tempdir().unwrap();
        let transaction = begin(root.path()).unwrap();
        transaction.prepare(vec![change("a", "new")]).unwrap();
        drop(transaction);
        let transaction = begin(root.path()).unwrap();
        assert!(!root.path().join("a").exists());
        let mut journal = transaction.prepare(vec![change("a", "new")]).unwrap();
        transaction.apply_one(&mut journal, 0).unwrap();
        journal.phase = Phase::Committed;
        save(root.path(), &journal).unwrap();
        drop(transaction);
        assert!(begin(root.path()).unwrap().recovered);
        assert_eq!(std::fs::read(root.path().join("a")).unwrap(), b"new");
    }

    #[test]
    fn modified_backup_refuses_destructive_recovery_and_lock_is_exclusive() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a"), "old").unwrap();
        let transaction = begin(root.path()).unwrap();
        assert!(begin(root.path()).is_err());
        let mut journal = transaction.prepare(vec![change("a", "new")]).unwrap();
        transaction.apply_one(&mut journal, 0).unwrap();
        std::fs::write(
            root.path().join(data_path(&journal.id, "backup", 0)),
            "tampered",
        )
        .unwrap();
        drop(transaction);
        assert!(begin(root.path()).is_err());
        assert_eq!(std::fs::read(root.path().join("a")).unwrap(), b"new");
        assert!(root.path().join(JOURNAL).exists());
    }

    #[test]
    fn planning_snapshot_rejects_edits_during_network_or_dependency_preparation() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("package.json"), "reviewed").unwrap();
        let transaction = begin(root.path()).unwrap();
        let plan = change("package.json", "resolved")
            .observe(root.path())
            .unwrap();
        std::fs::write(root.path().join("package.json"), "user-edit").unwrap();
        assert!(transaction.commit(vec![plan]).is_err());
        assert_eq!(
            std::fs::read(root.path().join("package.json")).unwrap(),
            b"user-edit"
        );
        assert!(!root.path().join(JOURNAL).exists());
    }

    #[cfg(unix)]
    #[test]
    fn traversal_symlinks_and_case_collisions_are_rejected_before_mutation() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), root.path().join("linked")).unwrap();
        for plan in [
            vec![change("../escape", "x")],
            vec![change("linked/file", "x")],
            vec![change("Name", "x"), change("name", "y")],
        ] {
            assert!(begin(root.path()).unwrap().commit(plan).is_err());
        }
        assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());
        assert!(!root.path().join("Name").exists());
    }

    #[cfg(windows)]
    #[test]
    fn windows_file_lock_refuses_entire_plan_before_first_mutation() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a"), "old-a").unwrap();
        std::fs::write(root.path().join("b"), "old-b").unwrap();
        let _locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(root.path().join("b"))
            .unwrap();
        assert!(
            begin(root.path())
                .unwrap()
                .commit(vec![change("a", "new-a"), change("b", "new-b")])
                .is_err()
        );
        assert_eq!(std::fs::read(root.path().join("a")).unwrap(), b"old-a");
    }
}
