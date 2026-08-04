# Security policy

## Supported versions

Dofus-tab is at v0 and there is no maintenance branch: fixes land on `main` and
go out in the next release. Please check against `main` before reporting.

## Reporting a vulnerability

Use GitHub's **Report a vulnerability** button under the Security tab, which
opens a private advisory. If that is unavailable to you, write to
<Robertojoselozada@gmail.com> with `dofus-tab` in the subject.

Please do not open a public issue for anything that lets one program read
another's data or take control of a session. Everything else — crashes, wrong
window focused, a shortcut that does not fire — is an ordinary bug and belongs
in the issue tracker where other players can find it.

Expect a first reply within a week. This is a spare-time project, so a fix may
take longer than an acknowledgement.

## What is in scope

Dofus-tab runs as your own user, with no elevated privileges and no network
access of any kind. The interesting surface is small and mostly about what it
reads:

- **The Ankama Launcher settings file.** It holds the login e-mail and session
  material alongside the account nicknames. Dofus-tab reads only `id`,
  `nickname` and `tag`, never writes to it, and none of it should ever reach a
  log line or a window title. A path by which secrets from that file escape the
  process is a vulnerability.
- **Other processes' command lines**, read to map a window to an account. Only
  the `-logFile` argument is used.
- **Its own configuration**, under `~/.config/dofus-tab`, which should never be
  written outside that directory or with permissions wider than the user.
- **The global shortcuts registration**, in that a bug letting another
  application take over or observe the registered keys would matter.

## What is not

- Anything requiring an attacker who already runs code as your user. At that
  point they can read the launcher's files directly.
- The game client and the Ankama Launcher themselves. Report those to Ankama.
- Account bans, or the terms of use. Dofus-tab does not touch the game; see the
  Scope section of the README.
