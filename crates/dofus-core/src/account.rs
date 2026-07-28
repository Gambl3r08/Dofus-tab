//! Ankama account value objects.

use std::fmt;

/// Stable Ankama account identifier.
///
/// This is the only identifier that survives a restart of the game, the
/// launcher and the machine, so it is the key everything else is stored under.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AccountId(pub u64);

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An account as published by the Ankama Launcher.
///
/// Only display fields are modelled on purpose: the launcher settings file also
/// contains the login e-mail and session material, which this project never
/// reads. See [`crate::launcher`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountEntry {
    pub id: AccountId,
    pub nickname: Option<String>,
    /// Ankama discriminator shown after the nickname, e.g. `1546`.
    pub tag: Option<String>,
}

impl AccountEntry {
    /// Nickname with its discriminator, e.g. `someone#1546`.
    ///
    /// Falls back to the bare account id when the launcher has no nickname for
    /// the account (happens for guest accounts and freshly added ones).
    pub fn display_name(&self) -> String {
        match (&self.nickname, &self.tag) {
            (Some(nick), Some(tag)) => format!("{nick}#{tag}"),
            (Some(nick), None) => nick.clone(),
            (None, _) => format!("account {}", self.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(nickname: Option<&str>, tag: Option<&str>) -> AccountEntry {
        AccountEntry {
            id: AccountId(62833850),
            nickname: nickname.map(str::to_owned),
            tag: tag.map(str::to_owned),
        }
    }

    #[test]
    fn display_name_joins_nickname_and_tag() {
        assert_eq!(
            entry(Some("totethedemon"), Some("1546")).display_name(),
            "totethedemon#1546"
        );
    }

    #[test]
    fn display_name_without_tag_is_the_bare_nickname() {
        assert_eq!(
            entry(Some("totethedemon"), None).display_name(),
            "totethedemon"
        );
    }

    #[test]
    fn display_name_falls_back_to_the_account_id() {
        assert_eq!(entry(None, None).display_name(), "account 62833850");
    }
}
