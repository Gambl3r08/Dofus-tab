// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! macOS backends.
//!
//! Only `window` exists so far. Global hotkeys are not implemented; see
//! [`hotkeys`] for what stands in the way.

pub mod hotkeys;
pub mod window;
