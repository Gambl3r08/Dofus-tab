// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

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
/// Both platforms need privileged-feeling APIs to read another process'
/// arguments — `NtQueryInformationProcess` plus `ReadProcessMemory` on the PEB
/// on Windows, `sysctl(KERN_PROCARGS2)` on macOS — and both have to get corner
/// cases right that are invisible until they break: WOW64 mismatches, access
/// rights, the undocumented layout of the argument block. A well-tested crate
/// does it instead.
#[cfg(any(target_os = "windows", target_os = "macos"))]
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

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
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
