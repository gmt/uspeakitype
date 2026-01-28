//! Input Method backend using zwp_input_method_v2 protocol via SCTK.
//!
//! Uses zwp_input_method_v2 protocol for text injection on compositors that support it.
//! This is the preferred backend for non-wlroots Wayland compositors (GNOME, KDE, etc.).

use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;

use anyhow::{anyhow, Result};

use super::TextInjector;

// SCTK reexports - NEVER use wayland_client directly
use smithay_client_toolkit::delegate_input_method;
use smithay_client_toolkit::reexports::client::globals::{registry_queue_init, GlobalListContents};
use smithay_client_toolkit::reexports::client::protocol::wl_registry::{
    Event as WlRegistryEvent, WlRegistry,
};
use smithay_client_toolkit::reexports::client::protocol::wl_seat::{Event as WlSeatEvent, WlSeat};
use smithay_client_toolkit::reexports::client::backend::WaylandError;
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, EventQueue, QueueHandle};
use smithay_client_toolkit::reexports::protocols_misc::zwp_input_method_v2::client::zwp_input_method_v2::ZwpInputMethodV2;
use smithay_client_toolkit::seat::input_method::{
    Active, InputMethod, InputMethodEventState, InputMethodHandler, InputMethodManager,
};

/// Internal state for Wayland event handling.
/// Uses interior mutability since InputMethodHandler takes `&self`.
struct State {
    /// Set to true when compositor sends Unavailable event (another IME active)
    unavailable: Cell<bool>,
    /// Set to true when input method is activated and ready for text injection
    activated: Cell<bool>,
    /// Surrounding text from compositor: (text, cursor_pos, anchor_pos)
    surrounding: RefCell<Option<(String, u32, u32)>>,
}

impl State {
    fn new() -> Self {
        Self {
            unavailable: Cell::new(false),
            activated: Cell::new(false),
            surrounding: RefCell::new(None),
        }
    }
}

impl InputMethodHandler for State {
    fn handle_done(
        &self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _input_method: &ZwpInputMethodV2,
        state: &InputMethodEventState,
    ) {
        self.activated
            .set(matches!(state.active, Active::Active { .. }));
        let surrounding = &state.surrounding;
        if !surrounding.text.is_empty() {
            log::debug!(
                "surrounding_text: {} bytes, cursor={}, anchor={}",
                surrounding.text.len(),
                surrounding.cursor,
                surrounding.anchor
            );
        }
        self.surrounding.borrow_mut().replace((
            surrounding.text.clone(),
            surrounding.cursor,
            surrounding.anchor,
        ));
    }

    fn handle_unavailable(
        &self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _input_method: &ZwpInputMethodV2,
    ) {
        self.unavailable.set(true);
    }
}

delegate_input_method!(State);

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: WlRegistryEvent,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // No-op: we use GlobalList for binding
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &WlSeat,
        _event: WlSeatEvent,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // No-op: we only use seat to create input method
    }
}

pub struct InputMethodInjector {
    connection: Connection,
    queue: EventQueue<State>,
    /// Wrapped in ManuallyDrop so we can drop it before flushing in Drop impl,
    /// ensuring the destroy request is actually sent to the compositor.
    input_method: ManuallyDrop<InputMethod>,
    state: State,
}

impl InputMethodInjector {
    fn pump_events(&mut self) -> Result<()> {
        self.connection.flush()?;

        if let Some(guard) = self.connection.prepare_read() {
            match guard.read() {
                Ok(_) => {}
                Err(WaylandError::Io(ref io_err))
                    if io_err.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(anyhow!("wayland read error: {}", e)),
            }
        }

        self.queue.dispatch_pending(&mut self.state)?;
        Ok(())
    }

    pub fn new() -> Result<Self> {
        let connection = Connection::connect_to_env().map_err(|_| anyhow!("connection failed"))?;

        let (globals, mut queue) = registry_queue_init::<State>(&connection)
            .map_err(|_| anyhow!("protocol unavailable"))?;
        let qh = queue.handle();

        let manager =
            InputMethodManager::bind(&globals, &qh).map_err(|_| anyhow!("protocol unavailable"))?;

        let seat: WlSeat = globals
            .bind(&qh, 1..=9, ())
            .map_err(|_| anyhow!("protocol unavailable: no seat"))?;

        let input_method = manager.get_input_method(&qh, &seat);

        let mut state = State::new();

        for _ in 0..3 {
            queue
                .roundtrip(&mut state)
                .map_err(|e| anyhow!("roundtrip failed: {}", e))?;
            if state.unavailable.get() {
                return Err(anyhow!("another IME active"));
            }
        }

        Ok(Self {
            connection,
            queue,
            input_method: ManuallyDrop::new(input_method),
            state,
        })
    }

    /// Get surrounding text from the input field.
    ///
    /// Returns `Some((text, cursor_pos, anchor_pos))` if surrounding text was received,
    /// or `None` if not yet available.
    pub fn get_surrounding_text(&self) -> Option<(String, u32, u32)> {
        self.state.surrounding.borrow().clone()
    }
}

impl TextInjector for InputMethodInjector {
    fn name(&self) -> &'static str {
        "input_method"
    }

    fn inject(&mut self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        self.pump_events()?;

        if !self.state.activated.get() {
            return Err(anyhow!("input method not activated"));
        }

        let text_with_space = format!("{} ", text);
        self.input_method.commit_string(text_with_space);
        self.input_method.commit();
        self.connection.flush()?;
        Ok(())
    }
}

impl Drop for InputMethodInjector {
    fn drop(&mut self) {
        // SAFETY: We own the InputMethod and it won't be used after this.
        // We must drop it manually before flushing so the destroy request
        // is queued, then flush to actually send it to the compositor.
        // Without this, the compositor (KDE/KWin) may be left with stale
        // IME state that can cause crashes in clipboard handling.
        unsafe {
            ManuallyDrop::drop(&mut self.input_method);
        }
        let _ = self.connection.flush();
    }
}
