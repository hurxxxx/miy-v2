//! Per-configuration credentials. Backend selection is persisted and never changed
//! for an existing entry merely because its OS keychain becomes unavailable.
//! Callers additionally serialize the complete refresh/read/update transaction.
use anyhow::{Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const MAX_SECRET: usize = 64 * 1024;
const MAX_RECORD: u64 = 512 * 1024;
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Backend {
    Native,
    UnixFile,
    WindowsDpapi,
}

// Intentionally no Debug: this record may contain plaintext on Unix.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    backend: Backend,
    value: Option<String>,
}

struct Store {
    record: PathBuf,
    namespace: String,
    key: String,
    _lock: File,
}

pub fn get(root: &Path, key: &str) -> Result<Option<String>> {
    Store::open(root, key)?.get()
}

pub fn set(root: &Path, key: &str, value: &str) -> Result<()> {
    let force_file = match std::env::var("MIY_CREDENTIAL_BACKEND") {
        Err(std::env::VarError::NotPresent) => false,
        Ok(value) if value == "file" => true,
        _ => bail!("credential_backend_invalid"),
    };
    Store::open(root, key)?.set(value, force_file)
}

pub fn remove(root: &Path, key: &str) -> Result<()> {
    Store::open(root, key)?.remove()
}

fn safe<T>(result: Result<T>) -> Result<T> {
    result.map_err(|_| anyhow!("credential_storage_failed"))
}

fn private_path(path: &Path, directory: bool) -> Result<()> {
    safe(crate::platform::reject_reparse(path))?;
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => bail!("credential_storage_unavailable"),
    };
    ensure!(
        if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        },
        "credential_storage_invalid_path"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        unsafe extern "C" {
            fn geteuid() -> u32;
        }
        ensure!(
            metadata.mode() & 0o077 == 0 && metadata.uid() == unsafe { geteuid() },
            "credential_storage_permissions"
        );
        if !directory {
            ensure!(metadata.nlink() == 1, "credential_storage_linked_file");
        }
    }
    Ok(())
}

impl Store {
    fn open(root: &Path, key: &str) -> Result<Self> {
        ensure!(
            key.len() == 64
                && key
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "credential_key_invalid"
        );
        safe(crate::platform::reject_reparse(root))?;
        let root = root
            .canonicalize()
            .map_err(|_| anyhow!("credential_root_unavailable"))?;
        let namespace = hex::encode(Sha256::digest(root.as_os_str().as_encoded_bytes()));
        let directory = root.join("credentials");
        safe(crate::platform::reject_reparse(&directory))?;
        let builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        match builder.create(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => bail!("credential_directory_unavailable"),
        }
        private_path(&directory, true)?;
        let lock_path = directory.join(format!("{key}.lock"));
        private_path(&lock_path, false)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options
            .open(&lock_path)
            .map_err(|_| anyhow!("credential_lock_unavailable"))?;
        private_path(&lock_path, false)?;
        let started = Instant::now();
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if started.elapsed() < LOCK_TIMEOUT => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(std::fs::TryLockError::WouldBlock) => bail!("credential_lock_busy"),
                Err(_) => bail!("credential_lock_failed"),
            }
        }
        let record = directory.join(format!("{key}.json"));
        private_path(&record, false)?;
        Ok(Self {
            record,
            namespace,
            key: key.into(),
            _lock: lock,
        })
    }

    fn read(&self) -> Result<Option<Record>> {
        private_path(&self.record, false)?;
        let file = match File::open(&self.record) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => bail!("credential_read_failed"),
        };
        let mut bytes = Vec::new();
        file.take(MAX_RECORD + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| anyhow!("credential_read_failed"))?;
        ensure!(
            bytes.len() as u64 <= MAX_RECORD,
            "credential_record_too_large"
        );
        let record: Record =
            serde_json::from_slice(&bytes).map_err(|_| anyhow!("credential_record_invalid"))?;
        ensure!(record.version == 1, "credential_record_version_unsupported");
        ensure!(
            matches!(record.backend, Backend::Native) == record.value.is_none(),
            "credential_record_invalid"
        );
        Ok(Some(record))
    }

    fn write(&self, record: &Record) -> Result<()> {
        private_path(&self.record, false)?;
        let bytes = serde_json::to_vec(record).map_err(|_| anyhow!("credential_record_invalid"))?;
        safe(crate::platform::write_private(&self.record, &bytes))
    }

    fn get(&self) -> Result<Option<String>> {
        let Some(record) = self.read()? else {
            return Ok(None);
        };
        let value = match record.backend {
            Backend::Native => native::get(&self.namespace, &self.key)?,
            Backend::UnixFile => {
                ensure!(cfg!(unix), "credential_backend_unsupported");
                record.value
            }
            Backend::WindowsDpapi => {
                #[cfg(windows)]
                {
                    use base64::Engine;
                    let encoded = record
                        .value
                        .ok_or_else(|| anyhow!("credential_record_invalid"))?;
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .map_err(|_| anyhow!("credential_record_invalid"))?;
                    Some(native::unprotect(&bytes, &self.namespace, &self.key)?)
                }
                #[cfg(not(windows))]
                bail!("credential_backend_unsupported")
            }
        };
        ensure!(
            value.as_ref().is_none_or(|value| value.len() <= MAX_SECRET),
            "credential_value_too_large"
        );
        Ok(value)
    }

    fn file_record(&self, value: &str) -> Result<Record> {
        #[cfg(windows)]
        {
            use base64::Engine;
            Ok(Record {
                version: 1,
                backend: Backend::WindowsDpapi,
                value: Some(
                    base64::engine::general_purpose::STANDARD.encode(native::protect(
                        value,
                        &self.namespace,
                        &self.key,
                    )?),
                ),
            })
        }
        #[cfg(unix)]
        {
            Ok(Record {
                version: 1,
                backend: Backend::UnixFile,
                value: Some(value.into()),
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            bail!("credential_backend_unsupported")
        }
    }

    fn set(&self, value: &str, force_file: bool) -> Result<()> {
        ensure!(value.len() <= MAX_SECRET, "credential_value_too_large");
        if let Some(record) = self.read()? {
            match record.backend {
                Backend::Native => {
                    // A timeout/outage never moves an existing credential to a
                    // different backend, even if an override has since changed.
                    native::set(&self.namespace, &self.key, value)
                        .map_err(|_| anyhow!("credential_keychain_write_failed"))?;
                }
                Backend::UnixFile => {
                    ensure!(cfg!(unix), "credential_backend_unsupported");
                    self.write(&self.file_record(value)?)?;
                }
                Backend::WindowsDpapi => {
                    ensure!(cfg!(windows), "credential_backend_unsupported");
                    self.write(&self.file_record(value)?)?;
                }
            }
            return Ok(());
        }
        if force_file {
            return self.write(&self.file_record(value)?);
        }
        // A read-only probe distinguishes an unavailable/locked OS service
        // before any write is attempted. This is also safe when secret-tool and
        // a session bus exist but no Secret Service is installed. Existing
        // records above never take this fallback, nor do uncertain writes below.
        if native::get(&self.namespace, &self.key).is_err() {
            return self.write(&self.file_record(value)?);
        }
        // Persist routing before the external operation. If its outcome becomes
        // uncertain, subsequent reads/retries still use this same keychain entry.
        self.write(&Record {
            version: 1,
            backend: Backend::Native,
            value: None,
        })?;
        match native::set(&self.namespace, &self.key, value) {
            Ok(()) => Ok(()),
            Err(NativeWriteError::Unavailable) => self.write(&self.file_record(value)?),
            Err(NativeWriteError::Failed) => bail!("credential_keychain_write_failed"),
        }
    }

    fn remove(&self) -> Result<()> {
        let Some(record) = self.read()? else {
            return Ok(());
        };
        if matches!(record.backend, Backend::Native) {
            native::remove(&self.namespace, &self.key)?;
        }
        // Keep the marker on remote deletion failure so retry still targets the
        // correct backend. A crash after deletion is an idempotent retry.
        private_path(&self.record, false)?;
        std::fs::remove_file(&self.record).map_err(|_| anyhow!("credential_remove_failed"))
    }
}

#[derive(Debug)]
enum NativeWriteError {
    // Only a definite unavailable/unsupported operation can choose a fallback.
    Unavailable,
    Failed,
}

#[cfg(all(unix, not(target_os = "macos")))]
mod native {
    use super::*;
    use base64::Engine;
    use std::{
        io::Write,
        os::unix::process::CommandExt,
        process::{Command, Stdio},
        sync::mpsc,
    };

    const TIMEOUT: Duration = Duration::from_secs(5);

    fn tool() -> std::result::Result<PathBuf, NativeWriteError> {
        if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
            return Err(NativeWriteError::Unavailable);
        }
        crate::platform::executable("secret-tool").map_err(|_| NativeWriteError::Unavailable)
    }

    fn run(
        arguments: &[&str],
        input: Option<Vec<u8>>,
        allow_missing: bool,
    ) -> std::result::Result<Option<Vec<u8>>, NativeWriteError> {
        process(&tool()?, arguments, input, allow_missing, TIMEOUT)
    }

    fn process(
        tool: &Path,
        arguments: &[&str],
        input: Option<Vec<u8>>,
        allow_missing: bool,
        timeout: Duration,
    ) -> std::result::Result<Option<Vec<u8>>, NativeWriteError> {
        let mut child = Command::new(tool)
            .args(arguments)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|_| NativeWriteError::Failed)?;
        // Drain concurrently so neither a full stdout pipe nor an unread stdin
        // can defeat the deadline. Never include captured output in errors.
        let stdout = child.stdout.take().ok_or(NativeWriteError::Failed)?;
        let (reader_tx, reader) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let result = capture(stdout);
            let _ = reader_tx.send(result);
        });
        let stderr = child.stderr.take().ok_or(NativeWriteError::Failed)?;
        let (diagnostics_tx, diagnostics) = mpsc::sync_channel(1);
        thread::spawn(move || {
            // Only presence matters: secret-tool reports lookup/clear absence
            // with exit 1 and no diagnostic. Do not inspect error wording.
            let _ = diagnostics_tx.send(capture(stderr).map(|bytes| !bytes.is_empty()));
        });
        let writer = input.map(|input| {
            let stdin = child.stdin.take();
            let (writer_tx, writer) = mpsc::sync_channel(1);
            thread::spawn(move || {
                let result = stdin
                    .ok_or_else(|| std::io::Error::other("credential_input_unavailable"))
                    .and_then(|mut stdin| stdin.write_all(&input));
                let _ = writer_tx.send(result);
            });
            writer
        });
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
                Ok(None) => {
                    // The still-live child owns this new process group. Never
                    // send a signal to a name or another user's keyring daemon.
                    unsafe extern "C" {
                        fn kill(pid: i32, signal: i32) -> i32;
                    }
                    unsafe {
                        kill(-(child.id() as i32), 9);
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let grace = Duration::from_millis(500);
        let written = writer.is_none_or(|writer| matches!(writer.recv_timeout(grace), Ok(Ok(()))));
        let output = reader
            .recv_timeout(grace)
            .map_err(|_| NativeWriteError::Failed)?
            .map_err(|_| NativeWriteError::Failed)?;
        let diagnostics = diagnostics
            .recv_timeout(grace)
            .map_err(|_| NativeWriteError::Failed)?
            .map_err(|_| NativeWriteError::Failed)?;
        let status = status.ok_or(NativeWriteError::Failed)?;
        if !written {
            return Err(NativeWriteError::Failed);
        }
        if status.success() {
            return Ok(Some(output));
        }
        if allow_missing && status.code() == Some(1) && !diagnostics && output.is_empty() {
            return Ok(None);
        }
        Err(NativeWriteError::Failed)
    }

    fn capture(mut output: impl Read) -> std::io::Result<Vec<u8>> {
        let mut buffer = Vec::new();
        let mut part = [0u8; 4096];
        loop {
            let count = output.read(&mut part)?;
            if count == 0 {
                break;
            }
            if buffer.len() + count <= MAX_RECORD as usize {
                buffer.extend_from_slice(&part[..count]);
            } else {
                return Err(std::io::Error::other("credential_output_too_large"));
            }
        }
        Ok::<_, std::io::Error>(buffer)
    }

    pub(super) fn get(namespace: &str, key: &str) -> Result<Option<String>> {
        let output = run(
            &[
                "lookup",
                "application",
                "miy-cli",
                "namespace",
                namespace,
                "key",
                key,
            ],
            None,
            true,
        )
        .map_err(|_| anyhow!("credential_keychain_read_failed"))?;
        let Some(output) = output else {
            return Ok(None);
        };
        let output = std::str::from_utf8(&output)
            .map_err(|_| anyhow!("credential_keychain_data_invalid"))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(output.trim_end_matches(['\r', '\n']))
            .map_err(|_| anyhow!("credential_keychain_data_invalid"))?;
        Ok(Some(String::from_utf8(bytes).map_err(|_| {
            anyhow!("credential_keychain_data_invalid")
        })?))
    }

    pub(super) fn set(
        namespace: &str,
        key: &str,
        value: &str,
    ) -> std::result::Result<(), NativeWriteError> {
        let mut input = base64::engine::general_purpose::STANDARD
            .encode(value)
            .into_bytes();
        // secret-tool's stdin reader has an 8192-byte limit. Larger records use
        // the protected file backend on first creation, never a truncated token.
        if input.len() >= 8192 {
            return Err(NativeWriteError::Unavailable);
        }
        input.push(b'\n');
        run(
            &[
                "store",
                "--label",
                "miy CLI",
                "application",
                "miy-cli",
                "namespace",
                namespace,
                "key",
                key,
            ],
            Some(input),
            false,
        )
        .map(|_| ())
    }

    pub(super) fn remove(namespace: &str, key: &str) -> Result<()> {
        run(
            &[
                "clear",
                "application",
                "miy-cli",
                "namespace",
                namespace,
                "key",
                key,
            ],
            None,
            true,
        )
        .map(|_| ())
        .map_err(|_| anyhow!("credential_keychain_remove_failed"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn helper_distinguishes_absence_from_failure_without_exposing_diagnostics() {
            let shell = Path::new("/bin/sh");
            assert!(
                process(shell, &["-c", "exit 1"], None, true, TIMEOUT)
                    .unwrap()
                    .is_none()
            );
            assert!(
                process(
                    shell,
                    &["-c", "echo secret-canary >&2; exit 1"],
                    None,
                    true,
                    TIMEOUT
                )
                .is_err()
            );
            let input = b"secret-only-on-stdin".to_vec();
            assert_eq!(
                process(shell, &["-c", "cat"], Some(input.clone()), false, TIMEOUT).unwrap(),
                Some(input)
            );
        }

        #[test]
        fn helper_timeout_bounds_an_unread_stdin_and_owned_child_tree() {
            let started = Instant::now();
            let result = process(
                Path::new("/bin/sh"),
                &["-c", "sleep 30"],
                Some(vec![b'x'; 128 * 1024]),
                false,
                Duration::from_millis(100),
            );
            assert!(result.is_err());
            assert!(started.elapsed() < Duration::from_secs(3));
        }

        #[test]
        fn oversized_secret_tool_input_is_rejected_before_spawning() {
            assert!(matches!(
                set("unused", "unused", &"x".repeat(8192)),
                Err(NativeWriteError::Unavailable)
            ));
        }
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::ptr;
    use windows_sys::Win32::{
        Foundation::{
            ERROR_ACCESS_DENIED, ERROR_NO_SUCH_LOGON_SESSION, ERROR_NOT_FOUND, ERROR_NOT_SUPPORTED,
            GetLastError, LocalFree,
        },
        Security::{
            Credentials::{
                CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
                CREDENTIALW, CredDeleteW, CredFree, CredReadW, CredWriteW,
            },
            Cryptography::{
                CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
            },
        },
    };

    fn target(namespace: &str, key: &str) -> Vec<u16> {
        format!("miy-cli/{namespace}/{key}")
            .encode_utf16()
            .chain([0])
            .collect()
    }

    pub(super) fn get(namespace: &str, key: &str) -> Result<Option<String>> {
        unsafe {
            let mut credential = ptr::null_mut();
            if CredReadW(
                target(namespace, key).as_ptr(),
                CRED_TYPE_GENERIC,
                0,
                &mut credential,
            ) == 0
            {
                if GetLastError() == ERROR_NOT_FOUND {
                    return Ok(None);
                }
                bail!("credential_keychain_read_failed");
            }
            struct Allocation(*mut CREDENTIALW);
            impl Drop for Allocation {
                fn drop(&mut self) {
                    unsafe {
                        CredFree(self.0.cast());
                    }
                }
            }
            let allocation = Allocation(credential);
            let value = &*allocation.0;
            ensure!(
                value.CredentialBlobSize as usize <= MAX_SECRET,
                "credential_value_too_large"
            );
            let bytes = if value.CredentialBlobSize == 0 {
                Vec::new()
            } else {
                ensure!(
                    !value.CredentialBlob.is_null(),
                    "credential_keychain_data_invalid"
                );
                std::slice::from_raw_parts(value.CredentialBlob, value.CredentialBlobSize as usize)
                    .to_vec()
            };
            Ok(Some(String::from_utf8(bytes).map_err(|_| {
                anyhow!("credential_keychain_data_invalid")
            })?))
        }
    }

    pub(super) fn set(
        namespace: &str,
        key: &str,
        value: &str,
    ) -> std::result::Result<(), NativeWriteError> {
        if value.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
            return Err(NativeWriteError::Unavailable);
        }
        let mut target = target(namespace, key);
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: value.len() as u32,
            CredentialBlob: value.as_ptr().cast_mut(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        if unsafe { CredWriteW(&credential, 0) } != 0 {
            Ok(())
        } else {
            match unsafe { GetLastError() } {
                ERROR_ACCESS_DENIED | ERROR_NOT_SUPPORTED | ERROR_NO_SUCH_LOGON_SESSION => {
                    Err(NativeWriteError::Unavailable)
                }
                _ => Err(NativeWriteError::Failed),
            }
        }
    }

    pub(super) fn remove(namespace: &str, key: &str) -> Result<()> {
        unsafe {
            ensure!(
                CredDeleteW(target(namespace, key).as_ptr(), CRED_TYPE_GENERIC, 0) != 0
                    || GetLastError() == ERROR_NOT_FOUND,
                "credential_keychain_remove_failed"
            );
        }
        Ok(())
    }

    fn crypt(bytes: &[u8], namespace: &str, key: &str, encrypt: bool) -> Result<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: bytes.len() as u32,
            pbData: bytes.as_ptr().cast_mut(),
        };
        // Bind ciphertext to this configuration and key as well as the current
        // Windows user. Never use CRYPTPROTECT_LOCAL_MACHINE.
        let entropy = format!("miy-cli/{namespace}/{key}");
        let entropy = CRYPT_INTEGER_BLOB {
            cbData: entropy.len() as u32,
            pbData: entropy.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        unsafe {
            let success = if encrypt {
                CryptProtectData(
                    &input,
                    ptr::null(),
                    &entropy,
                    ptr::null(),
                    ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    ptr::null_mut(),
                    &entropy,
                    ptr::null(),
                    ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            };
            ensure!(success != 0, "credential_dpapi_failed");
            struct Allocation(*mut u8);
            impl Drop for Allocation {
                fn drop(&mut self) {
                    unsafe {
                        LocalFree(self.0.cast());
                    }
                }
            }
            let allocation = Allocation(output.pbData);
            ensure!(
                output.cbData as u64 <= MAX_RECORD,
                "credential_value_too_large"
            );
            if output.cbData == 0 {
                return Ok(Vec::new());
            }
            ensure!(!allocation.0.is_null(), "credential_dpapi_failed");
            Ok(std::slice::from_raw_parts(allocation.0, output.cbData as usize).to_vec())
        }
    }

    pub(super) fn protect(value: &str, namespace: &str, key: &str) -> Result<Vec<u8>> {
        crypt(value.as_bytes(), namespace, key, true)
    }

    pub(super) fn unprotect(bytes: &[u8], namespace: &str, key: &str) -> Result<String> {
        String::from_utf8(crypt(bytes, namespace, key, false)?)
            .map_err(|_| anyhow!("credential_dpapi_data_invalid"))
    }
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use std::{ffi::c_void, ptr};

    type Cf = *const c_void;
    #[repr(C)]
    struct KeyCallbacks {
        version: isize,
        retain: Option<unsafe extern "C" fn(Cf, Cf) -> Cf>,
        release: Option<unsafe extern "C" fn(Cf, Cf)>,
        copy_description: Option<unsafe extern "C" fn(Cf) -> Cf>,
        equal: Option<unsafe extern "C" fn(Cf, Cf) -> u8>,
        hash: Option<unsafe extern "C" fn(Cf) -> usize>,
    }
    #[repr(C)]
    struct ValueCallbacks {
        version: isize,
        retain: Option<unsafe extern "C" fn(Cf, Cf) -> Cf>,
        release: Option<unsafe extern "C" fn(Cf, Cf)>,
        copy_description: Option<unsafe extern "C" fn(Cf) -> Cf>,
        equal: Option<unsafe extern "C" fn(Cf, Cf) -> u8>,
    }
    const NOT_FOUND: i32 = -25300;
    const UTF8: u32 = 0x0800_0100;
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(value: Cf);
        fn CFStringCreateWithBytes(
            allocator: Cf,
            bytes: *const u8,
            length: isize,
            encoding: u32,
            external: u8,
        ) -> Cf;
        fn CFDataCreate(allocator: Cf, bytes: *const u8, length: isize) -> Cf;
        fn CFDataGetLength(data: Cf) -> isize;
        fn CFDataGetBytePtr(data: Cf) -> *const u8;
        fn CFGetTypeID(value: Cf) -> usize;
        fn CFDataGetTypeID() -> usize;
        fn CFDictionaryCreate(
            allocator: Cf,
            keys: *const Cf,
            values: *const Cf,
            count: isize,
            key_callbacks: Cf,
            value_callbacks: Cf,
        ) -> Cf;
        static kCFBooleanTrue: Cf;
        static kCFTypeDictionaryKeyCallBacks: KeyCallbacks;
        static kCFTypeDictionaryValueCallBacks: ValueCallbacks;
    }
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        static kSecClass: Cf;
        static kSecClassGenericPassword: Cf;
        static kSecAttrService: Cf;
        static kSecAttrAccount: Cf;
        static kSecValueData: Cf;
        static kSecReturnData: Cf;
        static kSecMatchLimit: Cf;
        static kSecMatchLimitOne: Cf;
        static kSecUseAuthenticationUI: Cf;
        static kSecUseAuthenticationUIFail: Cf;
        fn SecItemCopyMatching(query: Cf, result: *mut Cf) -> i32;
        fn SecItemAdd(attributes: Cf, result: *mut Cf) -> i32;
        fn SecItemUpdate(query: Cf, attributes: Cf) -> i32;
        fn SecItemDelete(query: Cf) -> i32;
    }

    struct Owned(Cf);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe {
                CFRelease(self.0);
            }
        }
    }
    fn owned(value: Cf) -> Result<Owned> {
        ensure!(!value.is_null(), "credential_keychain_unavailable");
        Ok(Owned(value))
    }
    fn string(value: &str) -> Result<Owned> {
        owned(unsafe {
            CFStringCreateWithBytes(ptr::null(), value.as_ptr(), value.len() as isize, UTF8, 0)
        })
    }
    fn dictionary(pairs: &[(Cf, Cf)]) -> Result<Owned> {
        let keys: Vec<_> = pairs.iter().map(|pair| pair.0).collect();
        let values: Vec<_> = pairs.iter().map(|pair| pair.1).collect();
        owned(unsafe {
            CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                pairs.len() as isize,
                (&raw const kCFTypeDictionaryKeyCallBacks).cast(),
                (&raw const kCFTypeDictionaryValueCallBacks).cast(),
            )
        })
    }
    fn query(service: &Owned, account: &Owned) -> Vec<(Cf, Cf)> {
        unsafe {
            vec![
                (kSecClass, kSecClassGenericPassword),
                (kSecAttrService, service.0),
                (kSecAttrAccount, account.0),
                (kSecUseAuthenticationUI, kSecUseAuthenticationUIFail),
            ]
        }
    }

    pub(super) fn get(namespace: &str, key: &str) -> Result<Option<String>> {
        let service = string(&format!("miy-cli/{namespace}"))?;
        let account = string(key)?;
        let mut pairs = query(&service, &account);
        unsafe {
            pairs.extend([
                (kSecReturnData, kCFBooleanTrue),
                (kSecMatchLimit, kSecMatchLimitOne),
            ]);
        }
        let query = dictionary(&pairs)?;
        let mut result = ptr::null();
        let status = unsafe { SecItemCopyMatching(query.0, &mut result) };
        if status == NOT_FOUND {
            return Ok(None);
        }
        ensure!(status == 0, "credential_keychain_read_failed");
        let result = owned(result)?;
        unsafe {
            ensure!(
                CFGetTypeID(result.0) == CFDataGetTypeID(),
                "credential_keychain_data_invalid"
            );
            let length = CFDataGetLength(result.0);
            ensure!(
                (0..=MAX_SECRET as isize).contains(&length),
                "credential_value_too_large"
            );
            let bytes = if length == 0 {
                Vec::new()
            } else {
                let data = CFDataGetBytePtr(result.0);
                ensure!(!data.is_null(), "credential_keychain_data_invalid");
                std::slice::from_raw_parts(data, length as usize).to_vec()
            };
            Ok(Some(String::from_utf8(bytes).map_err(|_| {
                anyhow!("credential_keychain_data_invalid")
            })?))
        }
    }

    pub(super) fn set(
        namespace: &str,
        key: &str,
        value: &str,
    ) -> std::result::Result<(), NativeWriteError> {
        let result = (|| -> Result<i32> {
            let service = string(&format!("miy-cli/{namespace}"))?;
            let account = string(key)?;
            let data =
                owned(unsafe { CFDataCreate(ptr::null(), value.as_ptr(), value.len() as isize) })?;
            let mut pairs = query(&service, &account);
            let query = dictionary(&pairs)?;
            let attributes = dictionary(&[(unsafe { kSecValueData }, data.0)])?;
            let status = unsafe { SecItemUpdate(query.0, attributes.0) };
            if status != NOT_FOUND {
                return Ok(status);
            }
            pairs.push((unsafe { kSecValueData }, data.0));
            Ok(unsafe { SecItemAdd(dictionary(&pairs)?.0, ptr::null_mut()) })
        })();
        match result {
            Ok(0) => Ok(()),
            // errSecNotAvailable, errSecNoSuchKeychain, errSecInteractionNotAllowed.
            Ok(-25291 | -25294 | -25308) => Err(NativeWriteError::Unavailable),
            _ => Err(NativeWriteError::Failed),
        }
    }

    pub(super) fn remove(namespace: &str, key: &str) -> Result<()> {
        let service = string(&format!("miy-cli/{namespace}"))?;
        let account = string(key)?;
        let query = dictionary(&query(&service, &account))?;
        let status = unsafe { SecItemDelete(query.0) };
        ensure!(
            status == 0 || status == NOT_FOUND,
            "credential_keychain_remove_failed"
        );
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
mod native {
    use super::*;
    pub(super) fn get(_: &str, _: &str) -> Result<Option<String>> {
        bail!("credential_backend_unsupported")
    }
    pub(super) fn set(_: &str, _: &str, _: &str) -> std::result::Result<(), NativeWriteError> {
        Err(NativeWriteError::Unavailable)
    }
    pub(super) fn remove(_: &str, _: &str) -> Result<()> {
        bail!("credential_backend_unsupported")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(value: &str) -> String {
        hex::encode(Sha256::digest(value))
    }

    #[test]
    fn file_roundtrip_update_remove_and_root_isolation() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        let key = key("credential");
        let first = Store::open(one.path(), &key).unwrap();
        let second = Store::open(two.path(), &key).unwrap();
        assert_ne!(first.namespace, second.namespace);
        assert!(first.get().unwrap().is_none());
        first.set("한글\nsecret\0end", true).unwrap();
        assert_eq!(first.get().unwrap().as_deref(), Some("한글\nsecret\0end"));
        assert!(second.get().unwrap().is_none());
        first.set("rotated", false).unwrap();
        assert_eq!(first.get().unwrap().as_deref(), Some("rotated"));
        first.remove().unwrap();
        first.remove().unwrap();
        assert!(first.get().unwrap().is_none());
    }

    #[test]
    fn rejects_key_traversal_oversized_values_and_malformed_records_without_echo() {
        let root = tempfile::tempdir().unwrap();
        assert!(Store::open(root.path(), "../secret").is_err());
        let store = Store::open(root.path(), &key("entry")).unwrap();
        assert!(store.set(&"s".repeat(MAX_SECRET + 1), true).is_err());
        crate::platform::write_private(&store.record, b"not-json secret-canary").unwrap();
        let error = store.get().err().unwrap();
        assert_eq!(error.to_string(), "credential_record_invalid");
        assert!(!format!("{error:?}").contains("secret-canary"));
    }

    #[test]
    fn different_keys_have_independent_values() {
        let root = tempfile::tempdir().unwrap();
        let one = Store::open(root.path(), &key("one")).unwrap();
        let two = Store::open(root.path(), &key("two")).unwrap();
        one.set("first", true).unwrap();
        two.set("second", true).unwrap();
        one.remove().unwrap();
        assert_eq!(two.get().unwrap().as_deref(), Some("second"));
    }

    #[test]
    fn same_key_updates_are_serialized_and_records_stay_complete() {
        let root = tempfile::tempdir().unwrap();
        let key = key("shared");
        let threads: Vec<_> = (0..8)
            .map(|index| {
                let root = root.path().to_path_buf();
                let key = key.clone();
                thread::spawn(move || {
                    let store = Store::open(&root, &key).unwrap();
                    let value = format!("credential-{index}");
                    store.set(&value, true).unwrap();
                    assert_eq!(store.get().unwrap().as_ref(), Some(&value));
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert!(
            get(root.path(), &key)
                .unwrap()
                .unwrap()
                .starts_with("credential-")
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_fallback_permissions_and_links_are_rejected() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path(), &key("entry")).unwrap();
        store.set("test-secret", true).unwrap();
        assert_eq!(
            std::fs::metadata(store.record.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&store.record)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        std::fs::set_permissions(&store.record, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(store.get().is_err());
        std::fs::remove_file(&store.record).unwrap();
        let elsewhere = root.path().join("elsewhere");
        std::fs::write(&elsewhere, b"unchanged").unwrap();
        symlink(&elsewhere, &store.record).unwrap();
        assert!(store.set("replacement", true).is_err());
        assert_eq!(std::fs::read(elsewhere).unwrap(), b"unchanged");
    }

    #[cfg(windows)]
    #[test]
    fn windows_file_backend_is_dpapi_and_bound_to_root_and_key() {
        let root = tempfile::tempdir().unwrap();
        let first = Store::open(root.path(), &key("first")).unwrap();
        first.set("plaintext-canary", true).unwrap();
        let bytes = std::fs::read(&first.record).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("plaintext-canary"));
        let second = Store::open(root.path(), &key("second")).unwrap();
        crate::platform::write_private(&second.record, &bytes).unwrap();
        assert!(second.get().is_err());
    }
}
