//! Scope child trees to the command that owns them. Windows starts suspended so
//! a launcher cannot create descendants before joining its private Job Object.
use anyhow::Result;
use std::process::{Command, ExitStatus, Output};
#[cfg(not(windows))]
pub struct Guard;
#[cfg(windows)]
pub struct Guard(isize);
#[cfg(windows)]
impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0 as _);
        }
    }
}
#[cfg(windows)]
fn configure(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_SUSPENDED);
}
#[cfg(not(windows))]
fn configure(_: &mut Command) {}
#[cfg(windows)]
fn attach(handle: std::os::windows::io::RawHandle, pid: u32) -> Result<Guard> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
    };
    let error = || {
        crate::cli_error::CliError::new(
            crate::cli_error::Category::Environment,
            "process_ownership_unavailable",
            "Cannot establish ownership of the child process tree.",
        )
    };
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(error().into());
        }
        let guard = Guard(job as isize);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as _,
            std::mem::size_of_val(&limits) as u32,
        ) == 0
            || AssignProcessToJobObject(job, handle) == 0
        {
            return Err(error().into());
        }
        // std::process retains the process handle but closes the initial thread
        // handle. Its sole thread is still suspended; enumerate that owned PID.
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(error().into());
        }
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut found = Thread32First(snapshot, &mut entry) != 0;
        let mut resumed = false;
        while found {
            if entry.th32OwnerProcessID == pid {
                let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                if !thread.is_null() {
                    resumed = ResumeThread(thread) != u32::MAX;
                    CloseHandle(thread);
                }
                break;
            }
            found = Thread32Next(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
        if !resumed {
            return Err(error().into());
        }
        Ok(guard)
    }
}
fn standard(command: &mut Command) -> Result<(std::process::Child, Guard)> {
    configure(command);
    let child = command.spawn()?;
    #[cfg(windows)]
    let mut child = child;
    #[cfg(windows)]
    let guard = {
        use std::os::windows::io::AsRawHandle;
        match attach(child.as_raw_handle(), child.id()) {
            Ok(guard) => guard,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    };
    #[cfg(not(windows))]
    let guard = Guard;
    Ok((child, guard))
}
pub fn status(command: &mut Command) -> Result<ExitStatus> {
    let (mut child, _guard) = standard(command)?;
    Ok(child.wait()?)
}
pub fn output(command: &mut Command) -> Result<Output> {
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let (child, _guard) = standard(command)?;
    Ok(child.wait_with_output()?)
}
pub async fn spawn(
    command: &mut tokio::process::Command,
) -> Result<(tokio::process::Child, Guard)> {
    configure(command.as_std_mut());
    command.kill_on_drop(true);
    let child = command.spawn()?;
    #[cfg(windows)]
    let mut child = child;
    #[cfg(windows)]
    let guard = {
        let result = child
            .raw_handle()
            .zip(child.id())
            .ok_or_else(|| anyhow::anyhow!("child_handle_unavailable"))
            .and_then(|(handle, pid)| attach(handle, pid));
        match result {
            Ok(guard) => guard,
            Err(error) => {
                let _ = child.kill().await;
                return Err(error);
            }
        }
    };
    #[cfg(not(windows))]
    let guard = Guard;
    Ok((child, guard))
}
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    #[test]
    fn closing_job_terminates_only_owned_child() {
        let mut command = Command::new("cmd.exe");
        command
            .args(["/d", "/c", "ping -n 30 127.0.0.1 >nul"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let (mut child, guard) = standard(&mut command).unwrap();
        let (mut separate, separate_guard) = standard(&mut command).unwrap();
        let mut member = 0;
        unsafe {
            assert_ne!(
                windows_sys::Win32::System::JobObjects::IsProcessInJob(
                    child.as_raw_handle(),
                    guard.0 as _,
                    &mut member
                ),
                0
            );
        }
        assert_ne!(member, 0);
        assert!(child.try_wait().unwrap().is_none());
        assert!(separate.try_wait().unwrap().is_none());
        drop(guard);
        unsafe {
            assert_eq!(
                windows_sys::Win32::System::Threading::WaitForSingleObject(
                    child.as_raw_handle(),
                    3000
                ),
                0
            );
        }
        // KILL_ON_JOB_CLOSE guarantees termination, not a particular exit code.
        child.wait().unwrap();
        assert!(separate.try_wait().unwrap().is_none());
        drop(separate_guard);
        unsafe {
            assert_eq!(
                windows_sys::Win32::System::Threading::WaitForSingleObject(
                    separate.as_raw_handle(),
                    3000
                ),
                0
            );
        }
        separate.wait().unwrap();
    }
}
