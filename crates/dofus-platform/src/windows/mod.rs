// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Windows backends.
//!
//! `window` speaks Win32 directly; `hotkeys` uses `RegisterHotKey` with its own
//! message loop. Command lines go through `sysinfo` rather than hand-rolled
//! `NtQueryInformationProcess` plus `ReadProcessMemory`, which is the part of
//! this platform most likely to be subtly wrong (WOW64 mismatches, access
//! rights) and the least pleasant to debug.

pub mod hotkeys;
pub mod window;
