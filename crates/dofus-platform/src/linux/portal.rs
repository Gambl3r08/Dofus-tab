// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Global hotkeys through the XDG `GlobalShortcuts` desktop portal.
//!
//! The portal is used rather than an X11 `XGrabKey` because a grab on the X
//! server only fires while focus sits in an X11 or XWayland client. Dofus is
//! one, but the user's browser or chat client may not be, and a tab switcher
//! that stops working depending on which window is focused is worse than none.
//!
//! # Desktop support
//!
//! KDE Plasma implements this portal. GNOME's support has lagged behind, and
//! wlroots' `xdg-desktop-portal-wlr` ships no implementation at all, so
//! compositor-native fallbacks (Hyprland and Sway IPC) are still needed there.
//!
//! # Protocol
//!
//! Portal calls follow the request/response pattern: the method returns a
//! `Request` object path and the actual result arrives later as a `Response`
//! signal on it. To avoid missing a fast reply, the object path is predicted
//! from our unique bus name and subscribed to *before* issuing the call, as the
//! portal documentation prescribes.
//!
//! Reference: <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html>

use std::collections::HashMap;

use dofus_core::hotkey::{Action, HotkeyBinding, HotkeyManager, RegisteredShortcut};
use dofus_core::{Error, Result};
use zbus::blocking::{Connection, Proxy};
use zbus::names::UniqueName;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const GLOBAL_SHORTCUTS_INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
const REQUEST_INTERFACE: &str = "org.freedesktop.portal.Request";

/// Token identifying our shortcut session.
///
/// The portal persists bindings under this token, so keeping it constant is
/// what lets a user's customised triggers survive a restart.
const SESSION_TOKEN: &str = "dofus_tab";

/// Portal response code meaning the request completed.
const RESPONSE_SUCCESS: u32 = 0;
/// Portal response code meaning the user dismissed the dialog.
const RESPONSE_CANCELLED: u32 = 1;

/// Global hotkeys backed by the XDG desktop portal.
pub struct PortalHotkeys {
    connection: Connection,
    session: OwnedObjectPath,
    /// Incremented so every request gets a distinct object path.
    request_counter: u32,
}

impl PortalHotkeys {
    /// Opens a shortcuts session on the session bus.
    pub fn connect() -> Result<Self> {
        let connection = Connection::session().map_err(portal_error)?;
        let mut backend = Self {
            connection,
            session: default_session_path(),
            request_counter: 0,
        };
        backend.session = backend.create_session()?;
        Ok(backend)
    }

    fn portal(&self) -> Result<Proxy<'_>> {
        Proxy::new(
            &self.connection,
            PORTAL_DESTINATION,
            PORTAL_PATH,
            GLOBAL_SHORTCUTS_INTERFACE,
        )
        .map_err(portal_error)
    }

    /// Our unique bus name, formatted the way portal object paths encode it.
    fn sender_token(&self) -> Result<String> {
        let unique: UniqueName<'_> = self
            .connection
            .unique_name()
            .ok_or_else(|| Error::Backend("the session bus gave us no unique name".into()))?
            .as_ref();

        Ok(unique.as_str().trim_start_matches(':').replace('.', "_"))
    }

    /// Predicts the object path the portal will use for the next request.
    fn next_request(&mut self) -> Result<(String, OwnedObjectPath)> {
        self.request_counter += 1;
        let token = format!("dofus_tab_{}", self.request_counter);
        let sender = self.sender_token()?;
        let path = format!("{PORTAL_PATH}/request/{sender}/{token}");

        let path = ObjectPath::try_from(path)
            .map_err(portal_error)?
            .to_owned()
            .into();

        Ok((token, path))
    }

    /// Issues a portal call and blocks until its `Response` signal arrives.
    ///
    /// Subscribes to the response before making the call, so a reply that beats
    /// us cannot be missed.
    fn call_and_wait(
        &mut self,
        method: &str,
        body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
        request_path: OwnedObjectPath,
    ) -> Result<HashMap<String, OwnedValue>> {
        let request = Proxy::new(
            &self.connection,
            PORTAL_DESTINATION,
            request_path.clone(),
            REQUEST_INTERFACE,
        )
        .map_err(portal_error)?;

        let mut responses = request.receive_signal("Response").map_err(portal_error)?;

        self.portal()?
            .call_method(method, body)
            .map_err(portal_error)?;

        let message = responses.next().ok_or_else(|| {
            Error::Backend(format!("the portal closed the connection during {method}"))
        })?;

        let (code, results): (u32, HashMap<String, OwnedValue>) =
            message.body().deserialize().map_err(portal_error)?;

        match code {
            RESPONSE_SUCCESS => Ok(results),
            RESPONSE_CANCELLED => Err(Error::Backend(format!(
                "{method} was cancelled; the desktop asks for confirmation the first time \
                 shortcuts are registered"
            ))),
            other => Err(Error::Backend(format!(
                "{method} failed with portal response code {other}"
            ))),
        }
    }

    fn create_session(&mut self) -> Result<OwnedObjectPath> {
        let (request_token, request_path) = self.next_request()?;

        let options: HashMap<&str, Value<'_>> = HashMap::from([
            ("handle_token", Value::from(request_token)),
            ("session_handle_token", Value::from(SESSION_TOKEN)),
        ]);

        let results = self.call_and_wait("CreateSession", &(options,), request_path)?;

        let handle = results
            .get("session_handle")
            .ok_or_else(|| Error::Backend("the portal returned no session_handle".to_string()))?;

        // Portal versions disagree on whether this is an object path or a
        // string carrying one, so accept either.
        let path = match OwnedObjectPath::try_from(handle.try_clone().map_err(portal_error)?) {
            Ok(path) => path,
            Err(_) => {
                let raw: String = handle
                    .try_clone()
                    .map_err(portal_error)
                    .and_then(|value| String::try_from(value).map_err(portal_error))?;
                ObjectPath::try_from(raw)
                    .map_err(portal_error)?
                    .to_owned()
                    .into()
            }
        };

        tracing::debug!(session = %path.as_str(), "opened a global shortcuts session");
        Ok(path)
    }
}

impl HotkeyManager for PortalHotkeys {
    fn bind(&mut self, bindings: &[HotkeyBinding]) -> Result<()> {
        let shortcuts: Vec<(String, HashMap<&str, Value<'_>>)> = bindings
            .iter()
            .map(|binding| {
                let options = HashMap::from([
                    ("description", Value::from(binding.description.clone())),
                    ("preferred_trigger", Value::from(binding.trigger.clone())),
                ]);
                (binding.action.id(), options)
            })
            .collect();

        let (request_token, request_path) = self.next_request()?;
        let options: HashMap<&str, Value<'_>> =
            HashMap::from([("handle_token", Value::from(request_token))]);

        // The empty parent window identifier means "no parent"; the portal only
        // uses it to position its confirmation dialog.
        let body = (self.session.clone(), shortcuts, "", options);
        self.call_and_wait("BindShortcuts", &body, request_path)?;

        tracing::debug!(count = bindings.len(), "bound global shortcuts");
        Ok(())
    }

    fn registered(&mut self) -> Result<Vec<RegisteredShortcut>> {
        let (request_token, request_path) = self.next_request()?;
        let options: HashMap<&str, Value<'_>> =
            HashMap::from([("handle_token", Value::from(request_token))]);

        let body = (self.session.clone(), options);
        let results = self.call_and_wait("ListShortcuts", &body, request_path)?;

        let listed = results
            .get("shortcuts")
            .ok_or_else(|| Error::Backend("the portal returned no shortcut list".to_string()))?;

        let shortcuts: Vec<(String, HashMap<String, OwnedValue>)> =
            Vec::try_from(listed.try_clone().map_err(portal_error)?).map_err(portal_error)?;

        let mut registered: Vec<RegisteredShortcut> = shortcuts
            .into_iter()
            .filter_map(|(id, options)| {
                let action = Action::from_id(&id)?;
                Some(RegisteredShortcut {
                    action,
                    trigger: trigger_description(&options),
                })
            })
            .collect();

        // The portal answers in whatever order its own storage happens to use,
        // which is not slot order and looks like a bug to anyone reading it.
        registered.sort_by_key(|shortcut| shortcut.action);

        Ok(registered)
    }

    fn listen(&mut self, on_activated: &mut dyn FnMut(Action)) -> Result<()> {
        let portal = self.portal()?;
        let activations = portal.receive_signal("Activated").map_err(portal_error)?;

        for message in activations {
            let (session, shortcut_id, _timestamp, _options): (
                OwnedObjectPath,
                String,
                u64,
                HashMap<String, OwnedValue>,
            ) = match message.body().deserialize() {
                Ok(body) => body,
                Err(error) => {
                    tracing::debug!(%error, "ignoring an unparseable Activated signal");
                    continue;
                }
            };

            // The bus delivers signals for every session on this connection.
            if session != self.session {
                continue;
            }

            match Action::from_id(&shortcut_id) {
                Some(action) => on_activated(action),
                None => tracing::debug!(shortcut_id, "ignoring an unknown shortcut"),
            }
        }

        Err(Error::Backend(
            "the portal stopped delivering shortcut activations".to_string(),
        ))
    }
}

/// Reads the trigger a desktop reports for a shortcut.
///
/// An absent or empty `trigger_description` means the desktop accepted the
/// registration but bound no key to it.
fn trigger_description(options: &HashMap<String, OwnedValue>) -> Option<String> {
    let raw = options.get("trigger_description")?;
    let text = String::try_from(raw.try_clone().ok()?).ok()?;
    (!text.is_empty()).then_some(text)
}

/// Object path a session gets before the portal assigns the real one.
fn default_session_path() -> OwnedObjectPath {
    ObjectPath::from_static_str_unchecked(PORTAL_PATH).into()
}

fn portal_error(error: impl std::fmt::Display) -> Error {
    Error::Backend(error.to_string())
}
