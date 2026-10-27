//! OS boundaries shared by the local CLI and preview. No shell command strings.
use anyhow::{Context, Result, ensure};
#[cfg(not(windows))]
use std::process::Stdio;
use std::{
    ffi::{OsStr, OsString},
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

pub fn project_cli_relative() -> &'static str {
    if cfg!(windows) {
        ".miy-tools/miy.exe"
    } else {
        ".miy-tools/miy"
    }
}

pub fn project_cli(root: &Path) -> PathBuf {
    root.join(project_cli_relative())
}

fn cargo_bin() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|home| PathBuf::from(home).join(".cargo"))
        })
        .map(|root| root.join("bin"))
}

fn executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn executable_names(program: &str, extensions: Option<&OsStr>) -> Vec<OsString> {
    let Some(extensions) = extensions else {
        return vec![program.into()];
    };
    if Path::new(program).extension().is_some() {
        return vec![program.into()];
    }
    extensions
        .to_string_lossy()
        .split(';')
        .filter(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                ".exe" | ".com" | ".cmd" | ".bat"
            )
        })
        .map(|extension| format!("{program}{extension}").into())
        .collect()
}

pub(crate) fn executable_in(
    program: &str,
    path: &OsStr,
    cargo: Option<&Path>,
    extensions: Option<&OsStr>,
) -> Option<PathBuf> {
    let names = executable_names(program, extensions);
    let directories = std::env::split_paths(path).chain(
        cargo
            .filter(|_| matches!(program, "cargo" | "rustup" | "rustc"))
            .map(Path::to_path_buf),
    );
    for directory in directories {
        for name in &names {
            let candidate = directory.join(name);
            if executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn executable(program: &str) -> Result<PathBuf> {
    #[cfg(windows)]
    let extensions =
        Some(std::env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into()));
    #[cfg(not(windows))]
    let extensions: Option<OsString> = None;
    executable_in(
        program,
        &std::env::var_os("PATH").unwrap_or_default(),
        cargo_bin().as_deref(),
        extensions.as_deref(),
    )
    .with_context(|| {
        format!("missing_tool: {program}; run miy app doctor and install the required tool")
    })
}

pub fn tool_path() -> Result<OsString> {
    let mut paths: Vec<_> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    if let Some(path) = cargo_bin()
        && !paths.contains(&path)
    {
        paths.insert(0, path);
    }
    Ok(std::env::join_paths(paths)?)
}

fn validate_batch_argument(argument: &str) -> Result<()> {
    ensure!(
        !argument.chars().any(|c| matches!(
            c,
            '\0' | '\r' | '\n' | '"' | '%' | '!' | '^' | '&' | '|' | '<' | '>' | '(' | ')'
        )),
        "unsupported_windows_batch_argument: use a native executable or a path without shell metacharacters"
    );
    Ok(())
}

/// Rust handles the batch launcher quoting; reject shell-sensitive inputs before spawning it.
pub fn command(program: &str, args: &[&str]) -> Result<Command> {
    let path = executable(program)?;
    if cfg!(windows)
        && path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"))
    {
        validate_batch_argument(path.to_str().context("non_unicode_batch_path")?)?;
        for argument in args {
            validate_batch_argument(argument)?;
        }
    }
    let mut command = Command::new(path);
    command.args(args).env("PATH", tool_path()?);
    Ok(command)
}

pub fn open_browser(url: &str) -> Result<()> {
    let parsed = url::Url::parse(url)?;
    ensure!(
        matches!(parsed.scheme(), "http" | "https"),
        "unsupported_browser_url"
    );
    #[cfg(windows)]
    {
        windows::open_browser(url)
    }
    #[cfg(not(windows))]
    {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        Command::new(opener)
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    }
}

/// Reject a link at an owned path, including Windows junctions and other reparse points.
pub fn reject_reparse(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        !metadata.file_type().is_symlink(),
        "managed_path_is_link: {}",
        path.display()
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        ensure!(
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "managed_path_is_reparse_point: {}",
            path.display()
        );
    }
    Ok(())
}

fn parent(path: &Path) -> Result<&Path> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // Relative CLI paths belong to the current project. Inspect every owned
    // component before creating anything; OS-level aliases above cwd are allowed.
    let current = std::env::current_dir()?;
    let relative = if path.is_relative() {
        Some(path)
    } else {
        path.strip_prefix(&current).ok()
    };
    if let Some(relative) = relative {
        let mut component = current;
        for part in relative.components() {
            component.push(part);
            reject_reparse(&component)?;
        }
    }
    reject_reparse(parent)?;
    reject_reparse(path)?;
    std::fs::create_dir_all(parent)?;
    reject_reparse(parent)?;
    reject_reparse(path)?;
    Ok(parent)
}

fn create_private(path: &Path) -> Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        Ok(OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?)
    }
    #[cfg(windows)]
    {
        windows::create_private(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        anyhow::bail!("private_file_storage_unsupported")
    }
}

fn write_atomic(path: &Path, bytes: &[u8], private: bool) -> Result<()> {
    let directory = parent(path)?;
    let temporary = directory.join(format!(".miy-write-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = if private {
            create_private(&temporary)?
        } else {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?
        };
        file.write_all(bytes)?;
        file.sync_all()?;
        // Windows replacement requires closing our own handle first.
        drop(file);
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    write_atomic(path, bytes, true)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    reject_reparse(path)?;
    if std::fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    write_atomic(path, bytes, false)
}

pub fn install_executable(source: &Path, target: &Path) -> Result<()> {
    parent(target)?;
    let source = source.canonicalize()?;
    if target.canonicalize().ok().as_ref() == Some(&source) {
        return Ok(());
    }
    let temporary = target.with_file_name(format!(".miy-update-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        // Reserve a new path so pre-existing files or links are never followed.
        let mut destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut input = File::open(&source)?;
        std::io::copy(&mut input, &mut destination)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            destination.set_permissions(std::fs::Permissions::from_mode(0o755))?;
        }
        destination.sync_all()?;
        drop(destination);
        std::fs::rename(&temporary, target).context("project_cli_update_failed: close this project's running CLI/MCP processes and retry app setup")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        mem::size_of,
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle},
        },
        ptr,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, LocalFree},
        Security::{
            ACCESS_ALLOWED_ACE, ACE_HEADER,
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
            },
            DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetSecurityDescriptorControl,
            GetTokenInformation, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
            TokenUser,
        },
        Storage::FileSystem::{
            CREATE_NEW, CreateFileW, FILE_ALL_ACCESS, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ,
            READ_CONTROL,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
        UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };

    fn wide(value: &OsStr) -> Result<Vec<u16>> {
        let mut wide: Vec<_> = value.encode_wide().collect();
        ensure!(!wide.contains(&0), "nul_in_windows_argument");
        wide.push(0);
        Ok(wide)
    }

    struct Token(HANDLE);
    impl Drop for Token {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    struct Local(*mut core::ffi::c_void);
    impl Drop for Local {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }

    pub(super) fn create_private(path: &Path) -> Result<File> {
        // All access is granted only to the current user before the file exists.
        // A protected DACL prevents permissive parent ACLs from being inherited.
        unsafe {
            let mut token = ptr::null_mut();
            ensure!(
                OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) != 0,
                "private_file_token: {}",
                std::io::Error::last_os_error()
            );
            let token = Token(token);
            let mut length = 0;
            GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut length);
            ensure!(
                length >= size_of::<TOKEN_USER>() as u32,
                "private_file_token_size"
            );
            // usize allocation supplies alignment for TOKEN_USER and the trailing SID.
            let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
            ensure!(
                GetTokenInformation(
                    token.0,
                    TokenUser,
                    buffer.as_mut_ptr().cast(),
                    length,
                    &mut length
                ) != 0,
                "private_file_identity: {}",
                std::io::Error::last_os_error()
            );
            let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
            let mut sid = ptr::null_mut();
            ensure!(
                ConvertSidToStringSidW(user.User.Sid, &mut sid) != 0,
                "private_file_sid: {}",
                std::io::Error::last_os_error()
            );
            let _sid_memory = Local(sid.cast());
            let mut count = 0;
            while *sid.add(count) != 0 {
                count += 1;
            }
            let sid = String::from_utf16(std::slice::from_raw_parts(sid, count))?;
            let descriptor = wide(OsStr::new(&format!("O:{sid}D:P(A;;FA;;;{sid})")))?;
            let mut security = ptr::null_mut();
            ensure!(
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    descriptor.as_ptr(),
                    SDDL_REVISION_1,
                    &mut security,
                    ptr::null_mut()
                ) != 0,
                "private_file_acl: {}",
                std::io::Error::last_os_error()
            );
            let _security_memory = Local(security);
            let attributes = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: security,
                bInheritHandle: 0,
            };
            let path = wide(path.as_os_str())?;
            let handle = CreateFileW(
                path.as_ptr(),
                GENERIC_WRITE | READ_CONTROL,
                FILE_SHARE_READ,
                &attributes,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            );
            ensure!(
                handle != INVALID_HANDLE_VALUE,
                "private_file_create: {}",
                std::io::Error::last_os_error()
            );
            let file = File::from_raw_handle(handle);
            // Verify the filesystem honored the requested ACL before writing any secret.
            let mut actual = ptr::null_mut();
            let mut dacl = ptr::null_mut();
            let status = GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut dacl,
                ptr::null_mut(),
                &mut actual,
            );
            ensure!(status == 0, "private_file_acl_verification: {status}");
            let _actual_memory = Local(actual);
            let mut control = 0;
            let mut revision = 0;
            ensure!(
                GetSecurityDescriptorControl(actual, &mut control, &mut revision) != 0
                    && control & SE_DACL_PROTECTED != 0,
                "private_file_acl_not_protected"
            );
            ensure!(
                !dacl.is_null() && (*dacl).AceCount == 1,
                "private_file_acl_not_user_only"
            );
            let mut ace = ptr::null_mut();
            ensure!(GetAce(dacl, 0, &mut ace) != 0, "private_file_acl_entry");
            // ACCESS_ALLOWED_ACE_TYPE is zero; reject any other ACE layout before using its SID.
            ensure!(
                (*ace.cast::<ACE_HEADER>()).AceType == 0,
                "private_file_acl_entry_type"
            );
            let ace = &*ace.cast::<ACCESS_ALLOWED_ACE>();
            ensure!(
                ace.Mask == FILE_ALL_ACCESS
                    && EqualSid(ptr::addr_of!(ace.SidStart).cast_mut().cast(), user.User.Sid) != 0,
                "private_file_acl_not_user_only"
            );
            Ok(file)
        }
    }

    pub(super) fn open_browser(url: &str) -> Result<()> {
        let operation = wide(OsStr::new("open"))?;
        let url = wide(OsStr::new(url))?;
        let status = unsafe {
            ShellExecuteW(
                ptr::null_mut(),
                operation.as_ptr(),
                url.as_ptr(),
                ptr::null(),
                ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        ensure!(
            status as isize > 32,
            "browser_launch_failed: {}",
            status as isize
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_arguments_reject_shell_expansion() {
        for argument in [
            "hello world",
            "한글 경로",
            "--frozen-lockfile",
            "C:\\Users\\name\\project",
        ] {
            assert!(validate_batch_argument(argument).is_ok(), "{argument}");
        }
        for argument in [
            "%SECRET%", "!SECRET!", "a&whoami", "a|b", "a\nb", "a\"b", "a>b",
        ] {
            assert!(validate_batch_argument(argument).is_err(), "{argument}");
        }
    }

    #[test]
    fn executable_names_follow_pathext_without_shell_search() {
        assert_eq!(
            executable_names("pnpm", Some(OsStr::new(".EXE;.CMD;.PS1"))),
            vec![OsString::from("pnpm.EXE"), OsString::from("pnpm.CMD")]
        );
        assert_eq!(
            executable_names("miy.exe", Some(OsStr::new(".EXE;.CMD"))),
            vec![OsString::from("miy.exe")]
        );
    }

    #[test]
    fn private_replacement_and_failed_executable_replacement_preserve_files() {
        let root = tempfile::tempdir().unwrap();
        let secret = root.path().join("session.json");
        write_private(&secret, b"first").unwrap();
        write_private(&secret, b"second").unwrap();
        assert_eq!(std::fs::read(&secret).unwrap(), b"second");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&secret).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let existing = root.path().join("miy");
        std::fs::write(&existing, b"existing").unwrap();
        assert!(install_executable(&root.path().join("missing"), &existing).is_err());
        assert_eq!(std::fs::read(&existing).unwrap(), b"existing");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn locked_executable_replacement_preserves_the_installed_version() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("new.exe");
        let target = root.path().join("miy.exe");
        std::fs::write(&source, b"new version").unwrap();
        std::fs::write(&target, b"installed version").unwrap();
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&target)
            .unwrap();
        assert!(install_executable(&source, &target).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"installed version");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
        drop(locked);
        install_executable(&source, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new version");
    }
}
