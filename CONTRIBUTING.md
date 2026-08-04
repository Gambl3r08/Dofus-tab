# Contributing

Thanks for looking. This is a small project with a deliberately narrow purpose,
so the most useful thing to know first is what it will and will not become.

## Scope

Dofus-tab switches focus between windows, and names and orders them. Input
broadcasting, gameplay automation and reading game memory are out of scope
permanently — not merely unimplemented. Multi-accounting is allowed by Ankama;
those are not, and adding them would put users' accounts at risk. Pull requests
in that direction will be declined regardless of quality.

Everything else is fair game, and two things are actively wanted:

- **Running the untested backends on real hardware.** The Windows backend
  compiles and passes its tests on a Windows runner, but nobody has launched it
  next to an actual game client. A report saying what happened — even "it works"
  — is worth more right now than most patches.
- **Native Wayland backends** for compositors where the game does not go through
  XWayland, and an IPC fallback for the ones with no shortcuts portal (Hyprland,
  Sway). The README's Platform notes lay out what each one exposes.

## Getting set up

Rust 1.89 or newer, for the standard library file locking used by the
single-instance guard.

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

CI runs all three on Linux, Windows and macOS, and clippy warnings fail the
build, so run them before opening a pull request.

If you touch platform code you cannot run, type-check it for the other targets.
This catches most of what compiling would:

```sh
rustup target add x86_64-pc-windows-msvc aarch64-apple-darwin
cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings
cargo clippy --workspace --all-targets --target aarch64-apple-darwin -- -D warnings
```

## How the code is laid out

A hexagonal split, described at length under Architecture in the README. The
short version: `dofus-core` holds the domain model and the ports, and contains no
operating-system code; `dofus-platform` holds the adapters. Keeping that line
clean is what makes the ordering, naming and identity logic testable without a
display server, so new platform work belongs behind the existing traits rather
than reaching up into the core.

Tests follow from that: platform code is exercised through fakes. `Worker` takes
its window backend and process inspector as trait objects, and the parts that
touch the filesystem are split so their writing half can be pointed at a
throwaway directory. A patch that can only be verified by hand on your own
desktop is a patch that will break silently later.

## House style

- **English only** — code, comments, documentation, tests and commit messages.
- Comments explain *why*, not *what*. The existing ones record the reasons
  behind decisions that look arbitrary otherwise, such as why the app id has no
  hyphen or why hotkeys poll instead of blocking. That is the standard to match.
- Commit messages in the imperative mood, as in the existing history: "Add
  macOS AppKit backend", not "added" or "adding".
- One concern per pull request.

## Reporting bugs

Use the issue template — it asks for the output of `dofus-tab doctor` first,
because that one command separates the three failures that account for nearly
every report: the desktop never assigned the keys, the backend cannot see the
game, or the launcher changed how it starts clients.

## Licensing

Dofus-tab is GPL-3.0-or-later. By contributing you agree that your contribution
ships under the same terms.
