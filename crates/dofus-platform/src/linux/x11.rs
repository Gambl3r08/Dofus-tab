//! EWMH window backend, covering X11 sessions and XWayland clients.
//!
//! Everything here speaks the freedesktop [EWMH] specification, which every
//! mainstream window manager implements: `_NET_CLIENT_LIST` to enumerate,
//! `_NET_WM_PID` / `WM_CLASS` to identify, `_NET_ACTIVE_WINDOW` to focus.
//!
//! [EWMH]: https://specifications.freedesktop.org/wm-spec/latest/

use std::fmt::Display;

use dofus_core::{Error, GameWindow, Result, WindowId, WindowManager};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt, EventMask, PropMode,
    StackMode, Window,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;

/// `_NET_ACTIVE_WINDOW` source indication for a pager.
///
/// Window managers apply focus-stealing prevention to requests from regular
/// applications (source 1) but honour pager requests, which is exactly the role
/// a tab switcher plays.
const SOURCE_INDICATION_PAGER: u32 = 2;

/// Upper bound of property words to request; EWMH properties are far smaller.
const MAX_PROPERTY_WORDS: u32 = 4096;

/// `_NET_WM_STATE` action meaning "add this state".
const NET_WM_STATE_ADD: u32 = 1;

x11rb::atom_manager! {
    /// Atoms resolved once at connection time.
    pub Atoms: AtomsCookie {
        _NET_CLIENT_LIST,
        _NET_ACTIVE_WINDOW,
        _NET_WM_PID,
        _NET_WM_NAME,
        _NET_WM_STATE,
        _NET_WM_STATE_ABOVE,
        _NET_WM_STATE_SKIP_TASKBAR,
        _NET_WM_STATE_SKIP_PAGER,
        UTF8_STRING,
    }
}

/// Window manager backend talking EWMH over an X11 connection.
pub struct X11Backend {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
}

impl X11Backend {
    /// Connects to the X server named by `$DISPLAY`.
    ///
    /// Under a Wayland session this transparently reaches XWayland, which is
    /// where the Dofus client lives.
    pub fn connect() -> Result<Self> {
        let (conn, screen_num) = x11rb::connect(None).map_err(backend_error)?;
        let root = conn
            .setup()
            .roots
            .get(screen_num)
            .ok_or_else(|| Error::Backend(format!("screen {screen_num} does not exist")))?
            .root;
        let atoms = Atoms::new(&conn)
            .map_err(backend_error)?
            .reply()
            .map_err(backend_error)?;

        Ok(Self { conn, root, atoms })
    }

    /// Reads a `CARDINAL` property, returning its first word.
    fn cardinal(&self, window: Window, property: Atom) -> Option<u32> {
        self.property(window, property, AtomEnum::CARDINAL.into())?
            .value32()?
            .next()
    }

    /// Reads a text property, trying UTF-8 first and falling back to Latin-1.
    fn text(&self, window: Window, property: Atom, kind: Atom) -> Option<String> {
        let reply = self.property(window, property, kind)?;
        if reply.value.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(&reply.value).into_owned())
    }

    /// Issues a `GetProperty` request, treating any failure as "absent".
    ///
    /// Windows can be destroyed between the enumeration and these follow-up
    /// requests, so a `BadWindow` here is expected rather than exceptional.
    fn property(
        &self,
        window: Window,
        property: Atom,
        kind: Atom,
    ) -> Option<x11rb::protocol::xproto::GetPropertyReply> {
        self.conn
            .get_property(false, window, property, kind, 0, MAX_PROPERTY_WORDS)
            .ok()?
            .reply()
            .ok()
    }

    /// Reads `WM_CLASS`, which holds `instance\0class\0`.
    ///
    /// The class (second field) is what window managers report as the
    /// application class, e.g. `Dofus.x64`.
    fn window_class(&self, window: Window) -> String {
        let Some(reply) = self.property(window, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into())
        else {
            return String::new();
        };
        let mut fields = reply
            .value
            .split(|byte| *byte == 0)
            .filter(|field| !field.is_empty())
            .map(|field| String::from_utf8_lossy(field).into_owned());

        let instance = fields.next();
        fields.next().or(instance).unwrap_or_default()
    }

    /// Asks the window manager to add up to two `_NET_WM_STATE` properties.
    ///
    /// The spec allows two atoms per message, so callers batch related states.
    fn add_window_state(&self, window: Window, first: Atom, second: Atom) -> Result<()> {
        let event = ClientMessageEvent::new(
            32,
            window,
            self.atoms._NET_WM_STATE,
            // [action, first property, second property, source indication, unused]
            [NET_WM_STATE_ADD, first, second, SOURCE_INDICATION_PAGER, 0],
        );

        self.conn
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT,
                event,
            )
            .map_err(backend_error)?;

        Ok(())
    }

    /// Reads `_NET_WM_NAME`, falling back to the legacy `WM_NAME`.
    fn window_title(&self, window: Window) -> String {
        self.text(window, self.atoms._NET_WM_NAME, self.atoms.UTF8_STRING)
            .or_else(|| self.text(window, AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()))
            .unwrap_or_default()
    }
}

impl WindowManager for X11Backend {
    fn backend_name(&self) -> &'static str {
        "x11"
    }

    fn list_windows(&mut self) -> Result<Vec<GameWindow>> {
        let reply = self
            .conn
            .get_property(
                false,
                self.root,
                self.atoms._NET_CLIENT_LIST,
                AtomEnum::WINDOW,
                0,
                MAX_PROPERTY_WORDS,
            )
            .map_err(backend_error)?
            .reply()
            .map_err(backend_error)?;

        let handles: Vec<Window> = reply.value32().map(Iterator::collect).unwrap_or_default();

        // One round trip per property per window. With a handful of windows
        // this is well under a millisecond on a local socket; batch the cookies
        // here first if a future GUI ever polls this at frame rate.
        Ok(handles
            .into_iter()
            .map(|handle| GameWindow {
                id: WindowId(u64::from(handle)),
                pid: self.cardinal(handle, self.atoms._NET_WM_PID),
                title: self.window_title(handle),
                app_class: self.window_class(handle),
            })
            .collect())
    }

    fn active_window(&mut self) -> Result<Option<WindowId>> {
        let active = self
            .property(
                self.root,
                self.atoms._NET_ACTIVE_WINDOW,
                AtomEnum::WINDOW.into(),
            )
            .and_then(|reply| reply.value32()?.next())
            // A window manager with no focused window publishes 0 here.
            .filter(|handle| *handle != 0)
            .map(|handle| WindowId(u64::from(handle)));

        Ok(active)
    }

    fn make_overlay(&mut self, id: WindowId) -> Result<()> {
        let window = u32::try_from(id.0).map_err(|_| Error::WindowGone(id.0))?;

        let states = [
            self.atoms._NET_WM_STATE_ABOVE,
            self.atoms._NET_WM_STATE_SKIP_TASKBAR,
            self.atoms._NET_WM_STATE_SKIP_PAGER,
        ];

        // EWMH splits this in two: an unmapped window carries its states in the
        // property, a mapped one only changes them through a client message.
        // We cannot tell which side of the mapping we are on, so do both — the
        // window manager ignores whichever does not apply.
        self.conn
            .change_property32(
                PropMode::REPLACE,
                window,
                self.atoms._NET_WM_STATE,
                AtomEnum::ATOM,
                &states,
            )
            .map_err(backend_error)?;

        self.add_window_state(window, states[0], states[1])?;
        self.add_window_state(window, states[2], 0)?;

        self.conn.flush().map_err(backend_error)?;
        tracing::debug!(%id, "requested overlay window state");
        Ok(())
    }

    fn focus(&mut self, id: WindowId) -> Result<()> {
        let window = u32::try_from(id.0).map_err(|_| Error::WindowGone(id.0))?;

        let event = ClientMessageEvent::new(
            32,
            window,
            self.atoms._NET_ACTIVE_WINDOW,
            // [source indication, timestamp, requestor, unused, unused]
            [SOURCE_INDICATION_PAGER, x11rb::CURRENT_TIME, 0, 0, 0],
        );

        self.conn
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT,
                event,
            )
            .map_err(backend_error)?;

        // Best effort: some window managers focus without restacking. Failure is
        // harmless, so it must not turn a successful focus into an error.
        if let Err(error) = self.conn.configure_window(
            window,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        ) {
            tracing::debug!(%error, "could not raise the window, focus was still requested");
        }

        self.conn.flush().map_err(backend_error)?;
        Ok(())
    }
}

fn backend_error(error: impl Display) -> Error {
    Error::Backend(error.to_string())
}
