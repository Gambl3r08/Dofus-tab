# Dofus-tab

Cross-platform window switcher for playing Dofus with several accounts.

It maps every running Dofus client to a numbered tab and lets you jump between
them instantly. Unlike the existing tools it targets **Linux, Windows and
macOS** from a single codebase.

> **Status:** v0. On Linux the overlay bar, the command line interface, the X11
> backend and global hotkeys work end to end. The Windows backend is written and
> cross-compiles, but has not been run on Windows yet. macOS is not started —
> see [Roadmap](#roadmap).

## Scope

Dofus-tab only **switches focus between windows** and lets you name and order
them. It deliberately does not broadcast input to several clients, automate
gameplay, or read game memory. Multi-accounting is allowed by Ankama; automation
and input broadcasting are not.

## Install

Requires Rust 1.85 or newer.

```sh
cargo build --release
./target/release/dofus-tab list        # command line
./target/release/dofus-tab-bar         # overlay bar + hotkeys
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

## The overlay bar

```sh
cargo run --release --bin dofus-tab-bar
```

`dofus-tab-bar` is a thin always-on-top strip with one button per detected
client. The tab of the focused client is highlighted, clicking a tab focuses it,
and the dotted grip on the left drags the bar around — its position is
remembered in the config. The same process also registers the global hotkeys, so
one binary covers both ways of switching.

The bar re-scans every 750 ms on a worker thread, so the UI never blocks on an
X11 round trip, and clients launched or closed behind its back appear and
disappear on their own.

### Layout

Tabs are laid out in a grid that wraps, so eight accounts do not become a
thousand-pixel-wide strip. By default the bar fills **downwards and wraps after
four**, which keeps it narrow:

| Accounts | Default grid        |
|----------|---------------------|
| 2        | 1 column x 2 rows   |
| 4        | 1 column x 4 rows   |
| 5        | 2 columns x 4 rows  |
| 8        | 2 columns x 4 rows  |

Both the direction and the wrap point are configurable:

```json
{
  "overlay_orientation": "horizontal",
  "overlay_wrap_at": 4
}
```

`vertical` (the default) fills a column top to bottom before starting the next
one. `horizontal` fills a row left to right before starting the next one, so
eight accounts become 4 columns x 2 rows. Setting `overlay_wrap_at` above the
account count gives the old single-line bar back.

The window resizes itself to the grid, so it never reserves space it is not
using.

Two window-manager details worth knowing:

- The bar **forces itself onto X11** even in a Wayland session. Wayland has no
  standard way for an ordinary toplevel to stay above other windows, so a
  native-Wayland bar would sink behind the game. Going through XWayland lets the
  window manager honour `_NET_WM_STATE_ABOVE`, and costs nothing because the
  Dofus client is an XWayland client too.
- The always-on-top hints are applied by our own EWMH backend rather than by the
  GUI toolkit. winit maps `WindowLevel::AlwaysOnTop` to something KWin does not
  act on, so `make_overlay` sets `_NET_WM_STATE_ABOVE`, `SKIP_TASKBAR` and
  `SKIP_PAGER` directly. It both writes the property and sends the client
  message, because EWMH requires the former before the window is mapped and the
  latter after, and a caller cannot tell which side of the mapping it is on.

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
    windows/window.rs Win32 backend
    windows/hotkeys.rs global hotkeys via RegisterHotKey
    process.rs        reading another process' command line
  dofus-cli/        the command line interface
  dofus-gui/        the always-on-top bar (egui), plus the hotkey listener
    engine.rs         worker thread owning every platform handle
    app.rs            the bar itself
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

### Windows

Implemented but **not yet run on a Windows machine**; it is verified only by
cross-compilation (`cargo check --target x86_64-pc-windows-msvc`) and by the
unit tests that cover its pure logic. Treat it as untested until someone
launches it there.

Two differences from Linux are worth knowing:

- **The window class is useless.** Every Unity game reports `UnityWndClass`, so
  the backend fills `GameWindow::app_class` with the process image name
  (`Dofus.exe`) instead. That matches what X11 exposes through `WM_CLASS`, which
  is itself derived from the executable, so the shared filtering in `dofus-core`
  needs no platform branch.
- **Focus needs a handover.** Windows only lets the foreground process give
  focus away, so `focus` attaches its input queue to the current foreground
  thread with `AttachThreadInput` for the duration of the call. Minimised
  windows are restored first, since they cannot take focus otherwise.

Global hotkeys use `RegisterHotKey`, which needs no permission prompt but does
require registration and the message loop to run on the same thread — so
`bind` only records the bindings and `listen` performs the registration. A
combination already owned by another application fails to register and is
reported by `doctor` as having no trigger, the same way an inert portal shortcut
is on Linux.

Command lines go through `sysinfo` rather than hand-rolled
`NtQueryInformationProcess` plus `ReadProcessMemory` on the PEB. That is the
part of this platform most likely to be subtly wrong — WOW64 mismatches, access
rights — and the least pleasant to debug remotely.

### macOS

Not started. The account identification chain is expected to carry over
unchanged, because the launcher is the same Electron application on all three
platforms and stores its settings at `~/Library/Application Support/zaap/Settings`.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

Platform code that cannot be run locally is still type-checked and linted, which
catches most of what compiling would:

```sh
rustup target add x86_64-pc-windows-msvc
cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings
```

All code, comments, documentation and tests are written in English.

## Roadmap

- [x] `dofus-core` domain model with account identity and stable ordering
- [x] Linux EWMH backend (X11 and XWayland)
- [x] `dofus-tab` command line interface
- [x] Global hotkeys via the XDG `GlobalShortcuts` portal
- [ ] `.desktop` file and systemd user unit, so the portal sees our own app id
- [x] Always-on-top overlay bar
- [x] Windows backend (Win32) — written and cross-compiling, not yet run
- [ ] Run and fix the Windows backend on real hardware
- [ ] macOS backend (Accessibility API)
- [ ] Native Wayland backends (KWin, Hyprland, wlroots)

## License

MIT OR Apache-2.0
