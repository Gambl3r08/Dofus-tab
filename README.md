# Dofus-tab

Cross-platform window switcher for playing Dofus with several accounts.

It maps every running Dofus client to a numbered tab and lets you jump between
them instantly. Unlike the existing tools it targets **Linux, Windows and
macOS** from a single codebase.

> **Status:** v0. The command line interface, the Linux/X11 backend and global
> hotkeys work end to end. The overlay GUI and the Windows and macOS backends
> are not implemented yet — see [Roadmap](#roadmap).

## Scope

Dofus-tab only **switches focus between windows** and lets you name and order
them. It deliberately does not broadcast input to several clients, automate
gameplay, or read game memory. Multi-accounting is allowed by Ankama; automation
and input broadcasting are not.

## Install

Requires Rust 1.85 or newer.

```sh
cargo build --release
./target/release/dofus-tab list
```

## Usage

```sh
dofus-tab list             # show the detected clients as numbered tabs
dofus-tab focus 2          # focus a tab by slot
dofus-tab focus enutrof    # ...or by part of its label
dofus-tab label 1 "Iop"    # rename a tab (empty string restores the nickname)
dofus-tab save             # persist the current order across restarts
dofus-tab daemon           # register the global hotkeys and listen
dofus-tab doctor           # report what the platform layer detects
```

Example:

```
$ dofus-tab list
  1  someone#1546                  account=62833850     pid=11825    window=0x220000a
  2  Iop                           account=62845763     pid=12904    window=0x2400012
```

## Global hotkeys

`dofus-tab daemon` registers `Alt+1` … `Alt+8` and focuses the matching tab,
whichever window currently has focus.

Hotkeys are bound to **slots, not accounts**: `Alt+1` always focuses whatever is
in the first tab. That keeps muscle memory stable when an account is not logged
in, and it is what the portal requires — bindings are registered once, up front,
and cannot follow a changing window count. A fixed pool of eight is therefore
registered regardless of how many clients run; pressing a hotkey for an empty
slot does nothing.

Customise them in the config file (`~/.config/dofus-tab/config.json` on Linux):

```json
{
  "max_slots": 4,
  "hotkeys": {
    "1": "CTRL+F1",
    "2": "CTRL+F2"
  }
}
```

Triggers use the freedesktop shortcuts syntax (`ALT`, `CTRL`, `SHIFT`, `LOGO`,
joined with `+`). They are *preferred* triggers: the desktop may reassign one
that clashes with an existing shortcut, and the user can always change it
afterwards from their system settings.

### Why the portal, and not `XGrabKey`

An X11 key grab only fires while focus sits in an X11 or XWayland client. Dofus
is one, but a browser or chat client on a Wayland session is not, so a grab
would silently stop working depending on which window is focused. The XDG
`GlobalShortcuts` portal has no such blind spot.

The trade-off is desktop support: KDE Plasma implements the portal, GNOME's
support has lagged, and `xdg-desktop-portal-wlr` ships no implementation at all.
Hyprland and Sway will need their own IPC-based fallback.

### Troubleshooting: registered but inert

The desktop asks for confirmation the first time shortcuts are registered.
Declining that prompt puts KDE into a state worth knowing about:

- `BindShortcuts` fails immediately with portal response code 1.
- The shortcuts are nevertheless recorded, with the preferred trigger as their
  *default* and **no active key** (`focus-slot-1=none,Alt+1,…` in
  `kglobalshortcutsrc`).
- Later runs succeed without prompting again, because the entries already
  exist — so registration reports success while nothing ever fires.

`dofus-tab doctor` exists to make that visible: it calls `ListShortcuts` and
prints the trigger the desktop actually assigned to each action, flagging any
that read `NOT ASSIGNED`. `dofus-tab daemon` prints the same warning at startup.

The fix is to assign the keys once from the desktop's keyboard shortcut
settings. Deleting the `focus-slot-*` lines from `kglobalshortcutsrc` is not
enough on its own, and `kglobalaccel`'s `setShortcut` D-Bus method refuses to
assign keys to portal-owned actions.

> **Packaging note.** The portal derives the application identity from the
> systemd scope of the calling process, so running `dofus-tab daemon` from a
> terminal registers the shortcuts under *that terminal's* name in the desktop
> shortcut settings. Launching it from an installed `.desktop` file or a systemd
> user unit gives it its own identity.

## How accounts are identified

The Dofus 3 window title is a constant `"Dofus"` — it contains neither the
account nor the character name, so titles are useless for identification.

Dofus-tab goes through the Ankama Launcher instead. The launcher starts every
client with a per-account log file, and stores the account nicknames in its own
settings file:

```
X11 window
  -> _NET_WM_PID                                 -> 11825
  -> /proc/<pid>/cmdline, "-logFile" argument    -> dofus.62833850.log
  -> account id                                  -> 62833850
  -> <config>/zaap/Settings, USER_ACCOUNTS       -> someone#1546
```

The account id is the only identifier that survives a restart, so labels and tab
order are stored under it.

This relies on undocumented launcher behaviour and may break when Ankama ships
an update, so identification degrades gracefully: a user-assigned label wins over
the launcher nickname, and a window whose account cannot be resolved still gets a
tab.

**Privacy.** The launcher settings file also contains the login e-mail and
session material. Dofus-tab reads only `id`, `nickname` and `tag`, and never
writes to that file.

## Architecture

A hexagonal split, so the hard part — the operating system integration — stays
isolated and unit-testable:

```
crates/
  dofus-core/       domain model and ports (traits). No OS-specific code.
    account.rs        Ankama account value objects
    identity.rs       command line -> account id
    launcher.rs       Ankama Launcher settings -> nicknames
    config.rs         persisted labels and order
    registry.rs       windows + identity + config -> ordered tabs
  dofus-platform/   OS adapters implementing the ports
    linux/x11.rs      EWMH backend (X11 sessions and XWayland clients)
    linux/portal.rs   global hotkeys via the XDG GlobalShortcuts portal
    process.rs        reading another process' command line
  dofus-cli/        the v0 user interface
```

`dofus-core` defines two ports: `WindowManager` (enumerate and focus) and
`ProcessInspector` (read a command line). Every backend implements the same
traits, which keeps the ordering and naming logic testable without a display
server, and keeps the future GUI independent of the backend in use.

## Platform notes

### Linux

The Dofus 3 client is a Unity build whose SDL layer selects the X11 video
driver, so **the game runs through XWayland even inside a Wayland session**
(verified on KDE Plasma 6). The EWMH backend therefore covers both X11 and
Wayland desktops today.

That is a default, not a guarantee. A client started with `-force-wayland` or a
future Unity release would need a compositor-specific backend: KWin exposes
`workspace.windowList()` and `workspace.activeWindow` over D-Bus, Hyprland has
its IPC socket, and wlroots compositors implement
`wlr-foreign-toplevel-management`. GNOME is the hard case, as it offers no
window-control interface to third-party applications without a Shell extension.

Global hotkeys should go through the XDG `GlobalShortcuts` portal rather than
`XGrabKey`: an X11 grab only fires while focus sits in an X11 or XWayland
client, so it would miss keystrokes typed in native Wayland applications.

### Windows and macOS

Not implemented yet. The account identification chain is expected to carry over
unchanged, because the launcher is the same Electron application on all three
platforms and stores its settings at `%APPDATA%\zaap\Settings` and
`~/Library/Application Support/zaap/Settings` respectively.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

All code, comments, documentation and tests are written in English.

## Roadmap

- [x] `dofus-core` domain model with account identity and stable ordering
- [x] Linux EWMH backend (X11 and XWayland)
- [x] `dofus-tab` command line interface
- [x] Global hotkeys via the XDG `GlobalShortcuts` portal
- [ ] `.desktop` file and systemd user unit, so the portal sees our own app id
- [ ] Always-on-top overlay bar
- [ ] Windows backend (Win32)
- [ ] macOS backend (Accessibility API)
- [ ] Native Wayland backends (KWin, Hyprland, wlroots)

## License

MIT OR Apache-2.0
