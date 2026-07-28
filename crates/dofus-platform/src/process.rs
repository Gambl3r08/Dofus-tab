//! Reading the command line of another process.
//!
//! Only the Linux implementation exists so far, matching the v0 backend scope.
//! Windows needs `NtQueryInformationProcess` (or WMI) and macOS needs
//! `sysctl(KERN_PROCARGS2)`; both are tracked for the respective backends.

use dofus_core::{ProcessInspector, Result};

/// [`ProcessInspector`] backed by the host operating system.
pub struct OsProcessInspector;

impl ProcessInspector for OsProcessInspector {
    fn cmdline(&self, pid: u32) -> Result<Vec<String>> {
        cmdline(pid)
    }
}

#[cfg(target_os = "linux")]
pub fn cmdline(pid: u32) -> Result<Vec<String>> {
    use dofus_core::Error;

    let path = format!("/proc/{pid}/cmdline");
    let raw = std::fs::read(&path).map_err(|source| Error::io(&path, source))?;

    // `/proc/<pid>/cmdline` is a NUL-separated, NUL-terminated argument vector.
    Ok(raw
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect())
}

/// Reads a command line through `sysinfo`.
///
/// The Win32 way is `NtQueryInformationProcess` plus `ReadProcessMemory` on the
/// PEB, which has to get WOW64 mismatches and access rights right. That is
/// exactly the kind of code that fails subtly, so a well-tested crate does it
/// instead.
#[cfg(target_os = "windows")]
pub fn cmdline(pid: u32) -> Result<Vec<String>> {
    use dofus_core::Error;
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );

    let process = system
        .process(pid)
        .ok_or_else(|| Error::Backend(format!("process {pid} is gone")))?;

    Ok(process
        .cmd()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn cmdline(_pid: u32) -> Result<Vec<String>> {
    Err(dofus_core::Error::Unsupported(
        "reading a process command line",
    ))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn reads_our_own_command_line() {
        let pid = std::process::id();
        let args = cmdline(pid).expect("our own cmdline should be readable");
        assert!(!args.is_empty(), "argv[0] should always be present");
    }

    #[test]
    fn reports_an_error_for_a_process_that_does_not_exist() {
        assert!(cmdline(u32::MAX).is_err());
    }
}
