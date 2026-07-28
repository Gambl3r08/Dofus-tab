//! Windows backends.
//!
//! `window` speaks Win32 directly; `hotkeys` uses `RegisterHotKey` with its own
//! message loop. Command lines go through `sysinfo` rather than hand-rolled
//! `NtQueryInformationProcess` plus `ReadProcessMemory`, which is the part of
//! this platform most likely to be subtly wrong (WOW64 mismatches, access
//! rights) and the least pleasant to debug.

pub mod hotkeys;
pub mod window;
