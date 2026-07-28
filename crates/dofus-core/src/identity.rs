//! Recovering the Ankama account behind a running game client.
//!
//! The Dofus 3 window title is a constant `"Dofus"` and carries no character or
//! account name, so identification goes through the launcher instead. The
//! Ankama Launcher starts every client with a per-account log file:
//!
//! ```text
//! ./Dofus.x64 -logFile <config>/zaap/gamesLogs/dofus-dofus3/dofus.62833850.log ...
//! ```
//!
//! The number in that file name is the Ankama account id, which
//! [`crate::launcher`] then resolves to a nickname.
//!
//! This relies on undocumented launcher behaviour and may break on updates,
//! hence the caller is expected to fall back to user-assigned labels.

use crate::account::AccountId;

/// Argument the launcher uses to point the client at its log file.
const LOG_FILE_FLAG: &str = "-logFile";

/// Extracts the Ankama account id from a game client command line.
///
/// Accepts both `-logFile <path>` and `-logFile=<path>` spellings, and tolerates
/// the rotated `dofus.prev.<id>.log` name.
pub fn account_id_from_cmdline(args: &[String]) -> Option<AccountId> {
    log_file_path(args).and_then(account_id_from_log_name)
}

/// Returns the value of the `-logFile` argument, whichever spelling was used.
fn log_file_path(args: &[String]) -> Option<&str> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == LOG_FILE_FLAG {
            return iter.next().map(String::as_str);
        }
        if let Some(rest) = arg.strip_prefix(LOG_FILE_FLAG)
            && let Some(value) = rest.strip_prefix('=')
        {
            return Some(value);
        }
    }
    None
}

/// Parses `.../dofus.<id>.log` (or `dofus.prev.<id>.log`) into an account id.
fn account_id_from_log_name(path: &str) -> Option<AccountId> {
    // Split on both separators so a Windows path parses on Linux and vice
    // versa; this runs against paths produced by the launcher, not by us.
    let file_name = path.rsplit(['/', '\\']).next()?;
    let stem = file_name.strip_suffix(".log")?;
    let digits = stem
        .strip_prefix("dofus.prev.")
        .or_else(|| stem.strip_prefix("dofus."))?;

    digits.parse::<u64>().ok().map(AccountId)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn extracts_the_account_id_from_a_real_command_line() {
        let cmdline = args(&[
            "./Dofus.x64",
            "-logFile",
            "/home/user/.config/zaap/gamesLogs/dofus-dofus3/dofus.62833850.log",
            "--port",
            "26116",
            "--gameName",
            "dofus",
        ]);

        assert_eq!(account_id_from_cmdline(&cmdline), Some(AccountId(62833850)));
    }

    #[test]
    fn accepts_the_equals_spelling() {
        let cmdline = args(&["./Dofus.x64", "-logFile=/tmp/dofus.129165223.log"]);
        assert_eq!(
            account_id_from_cmdline(&cmdline),
            Some(AccountId(129165223))
        );
    }

    #[test]
    fn accepts_windows_style_paths() {
        let cmdline = args(&[
            "Dofus.exe",
            "-logFile",
            r"C:\Users\user\AppData\Roaming\zaap\gamesLogs\dofus-dofus3\dofus.62845763.log",
        ]);
        assert_eq!(account_id_from_cmdline(&cmdline), Some(AccountId(62845763)));
    }

    #[test]
    fn tolerates_the_rotated_log_name() {
        let cmdline = args(&["./Dofus.x64", "-logFile", "/tmp/dofus.prev.62833850.log"]);
        assert_eq!(account_id_from_cmdline(&cmdline), Some(AccountId(62833850)));
    }

    #[test]
    fn returns_none_without_a_log_file_argument() {
        assert_eq!(
            account_id_from_cmdline(&args(&["./Dofus.x64", "--port", "26116"])),
            None
        );
    }

    #[test]
    fn returns_none_when_the_flag_has_no_value() {
        assert_eq!(
            account_id_from_cmdline(&args(&["./Dofus.x64", "-logFile"])),
            None
        );
    }

    #[test]
    fn returns_none_for_an_unrelated_log_file() {
        let cmdline = args(&["./Dofus.x64", "-logFile", "/tmp/player.log"]);
        assert_eq!(account_id_from_cmdline(&cmdline), None);
    }

    #[test]
    fn returns_none_when_the_id_is_not_numeric() {
        let cmdline = args(&["./Dofus.x64", "-logFile", "/tmp/dofus.abc.log"]);
        assert_eq!(account_id_from_cmdline(&cmdline), None);
    }
}
