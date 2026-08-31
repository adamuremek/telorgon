use std::ffi::{CStr, CString, c_int, c_void};
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Duration;

use crate::wayland_server::ffi;
use crate::wayland_server::{WaylandServerError, WaylandServerErrorKind};

type ServerResult<T> = Result<T, WaylandServerError>;

pub struct Display {
    raw: NonNull<ffi::wl_display>,
    marker: PhantomData<Rc<()>>,
}

impl Display {
    pub fn new() -> ServerResult<Self> {
        let raw = unsafe { ffi::wl_display_create() };
        let raw = NonNull::new(raw).ok_or_else(|| {
            WaylandServerError::new(
                WaylandServerErrorKind::Allocation,
                "libwayland could not allocate the server display",
            )
        })?;
        Ok(Self {
            raw,
            marker: PhantomData,
        })
    }

    pub fn add_socket_auto(&self) -> ServerResult<String> {
        let name = unsafe { ffi::wl_display_add_socket_auto(self.raw.as_ptr()) };
        let name = NonNull::new(name.cast_mut()).ok_or_else(|| {
            WaylandServerError::new(
                WaylandServerErrorKind::Socket,
                "libwayland could not create a display socket",
            )
        })?;
        let name = unsafe { CStr::from_ptr(name.as_ptr()) };
        Ok(name.to_string_lossy().into_owned())
    }

    pub fn add_socket(&self, name: &str) -> ServerResult<()> {
        let name = CString::new(name).map_err(|_| {
            WaylandServerError::new(
                WaylandServerErrorKind::Socket,
                "Wayland socket name contains an interior NUL",
            )
        })?;
        let result = unsafe { ffi::wl_display_add_socket(self.raw.as_ptr(), name.as_ptr()) };
        native_zero(
            result,
            WaylandServerErrorKind::Socket,
            "libwayland could not create the requested display socket",
        )
    }

    pub fn event_loop(&self) -> EventLoopRef<'_> {
        let raw = unsafe { ffi::wl_display_get_event_loop(self.raw.as_ptr()) };
        EventLoopRef {
            raw: NonNull::new(raw).expect("a live Wayland display always owns an event loop"),
            marker: PhantomData,
        }
    }

    pub fn dispatch_pending(&self) -> ServerResult<()> {
        let result = unsafe { ffi::wl_display_dispatch_pending(self.raw.as_ptr()) };
        if result < 0 {
            Err(WaylandServerError::new(
                WaylandServerErrorKind::Dispatch,
                "libwayland failed to dispatch pending client requests",
            ))
        } else {
            Ok(())
        }
    }

    pub fn flush_clients(&self) {
        unsafe { ffi::wl_display_flush_clients(self.raw.as_ptr()) };
    }

    pub fn current_serial(&self) -> u32 {
        unsafe { ffi::wl_display_get_serial(self.raw.as_ptr()) }
    }

    pub fn next_serial(&self) -> u32 {
        unsafe { ffi::wl_display_next_serial(self.raw.as_ptr()) }
    }

    /// Native display identity for Telorgon-owned protocol layers that must allocate serials from
    /// the same libwayland sequence while handling a C callback.
    #[doc(hidden)]
    pub fn native_handle(&self) -> NonNull<ffi::wl_display> {
        self.raw
    }

    pub fn terminate(&self) {
        unsafe { ffi::wl_display_terminate(self.raw.as_ptr()) };
    }

    /// Creates a native global whose callback data and interface descriptor are borrowed from the
    /// caller.
    ///
    /// # Safety
    ///
    /// `interface`, `data`, and all storage reachable by `bind` must remain valid until the
    /// returned global is destroyed. The callback must obey the libwayland server ABI and must not
    /// unwind.
    pub unsafe fn create_global<'display>(
        &'display self,
        interface: &ffi::wl_interface,
        version: u32,
        data: *mut c_void,
        bind: ffi::wl_global_bind_func_t,
    ) -> ServerResult<Global<'display>> {
        if version == 0 || version > interface.version as u32 || version > c_int::MAX as u32 {
            return Err(WaylandServerError::new(
                WaylandServerErrorKind::InvalidVersion,
                "Wayland global version is outside the interface contract",
            ));
        }
        let raw = unsafe {
            ffi::wl_global_create(self.raw.as_ptr(), interface, version as c_int, data, bind)
        };
        Ok(Global {
            raw: NonNull::new(raw).ok_or_else(|| {
                WaylandServerError::new(
                    WaylandServerErrorKind::Allocation,
                    "libwayland could not allocate a global",
                )
            })?,
            marker: PhantomData,
        })
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        unsafe {
            ffi::wl_display_destroy_clients(self.raw.as_ptr());
            ffi::wl_display_destroy(self.raw.as_ptr());
        }
    }
}

pub struct EventLoopRef<'display> {
    raw: NonNull<ffi::wl_event_loop>,
    marker: PhantomData<&'display Display>,
}

impl EventLoopRef<'_> {
    pub fn dispatch(&self, timeout: Option<Duration>) -> ServerResult<()> {
        let timeout_ms = match timeout {
            None => -1,
            Some(duration) => i32::try_from(duration.as_millis()).map_err(|_| {
                WaylandServerError::new(
                    WaylandServerErrorKind::InvalidTimeout,
                    "Wayland event-loop timeout exceeds i32 milliseconds",
                )
            })?,
        };
        let result = unsafe { ffi::wl_event_loop_dispatch(self.raw.as_ptr(), timeout_ms) };
        if result < 0 {
            Err(WaylandServerError::new(
                WaylandServerErrorKind::Dispatch,
                "libwayland event-loop dispatch failed",
            ))
        } else {
            Ok(())
        }
    }

    pub fn dispatch_idle(&self) {
        unsafe { ffi::wl_event_loop_dispatch_idle(self.raw.as_ptr()) };
    }

    /// Registers an externally owned file descriptor with this event loop.
    ///
    /// # Safety
    ///
    /// `fd` must remain valid until the returned source is removed. `data` must remain valid for
    /// every callback, and `callback` must obey the libwayland ABI and must not unwind.
    pub unsafe fn add_fd(
        &self,
        fd: c_int,
        mask: u32,
        callback: ffi::wl_event_loop_fd_func_t,
        data: *mut c_void,
    ) -> ServerResult<EventSource> {
        let raw = unsafe { ffi::wl_event_loop_add_fd(self.raw.as_ptr(), fd, mask, callback, data) };
        EventSource::from_raw(raw)
    }

    /// Registers a timer callback with this event loop.
    ///
    /// # Safety
    ///
    /// `data` must remain valid until the returned source is removed. `callback` must obey the
    /// libwayland ABI and must not unwind.
    pub unsafe fn add_timer(
        &self,
        callback: ffi::wl_event_loop_timer_func_t,
        data: *mut c_void,
    ) -> ServerResult<EventSource> {
        let raw = unsafe { ffi::wl_event_loop_add_timer(self.raw.as_ptr(), callback, data) };
        EventSource::from_raw(raw)
    }
}

pub struct EventSource {
    raw: NonNull<ffi::wl_event_source>,
    marker: PhantomData<Rc<()>>,
}

impl EventSource {
    fn from_raw(raw: *mut ffi::wl_event_source) -> ServerResult<Self> {
        Ok(Self {
            raw: NonNull::new(raw).ok_or_else(|| {
                WaylandServerError::new(
                    WaylandServerErrorKind::Allocation,
                    "libwayland could not allocate an event source",
                )
            })?,
            marker: PhantomData,
        })
    }

    pub fn update_fd_mask(&self, mask: u32) -> ServerResult<()> {
        let result = unsafe { ffi::wl_event_source_fd_update(self.raw.as_ptr(), mask) };
        native_zero(
            result,
            WaylandServerErrorKind::NativeFailure,
            "libwayland could not update the event-source mask",
        )
    }

    pub fn arm_timer(&self, delay: Duration) -> ServerResult<()> {
        let delay_ms = i32::try_from(delay.as_millis()).map_err(|_| {
            WaylandServerError::new(
                WaylandServerErrorKind::InvalidTimeout,
                "Wayland timer delay exceeds i32 milliseconds",
            )
        })?;
        let result = unsafe { ffi::wl_event_source_timer_update(self.raw.as_ptr(), delay_ms) };
        native_zero(
            result,
            WaylandServerErrorKind::NativeFailure,
            "libwayland could not arm the event-source timer",
        )
    }
}

impl Drop for EventSource {
    fn drop(&mut self) {
        let result = unsafe { ffi::wl_event_source_remove(self.raw.as_ptr()) };
        debug_assert_eq!(result, 0, "libwayland event-source removal failed");
    }
}

pub struct Global<'display> {
    raw: NonNull<ffi::wl_global>,
    marker: PhantomData<&'display Display>,
}

impl Global<'_> {
    pub fn remove(&self) {
        unsafe { ffi::wl_global_remove(self.raw.as_ptr()) };
    }
}

impl Drop for Global<'_> {
    fn drop(&mut self) {
        unsafe { ffi::wl_global_destroy(self.raw.as_ptr()) };
    }
}

#[derive(Clone, Copy)]
pub struct ClientRef<'callback> {
    raw: NonNull<ffi::wl_client>,
    marker: PhantomData<&'callback mut ffi::wl_client>,
}

impl<'callback> ClientRef<'callback> {
    /// Borrows a client pointer supplied by a live libwayland callback.
    ///
    /// # Safety
    ///
    /// `raw` must either be null or identify a live `wl_client` for the entire `'callback`
    /// lifetime. The caller must prevent destruction while a returned reference is in use.
    pub unsafe fn from_raw(raw: *mut ffi::wl_client) -> Option<Self> {
        NonNull::new(raw).map(|raw| Self {
            raw,
            marker: PhantomData,
        })
    }

    pub fn credentials(self) -> ClientCredentials {
        let mut pid = 0;
        let mut uid = 0;
        let mut gid = 0;
        unsafe { ffi::wl_client_get_credentials(self.raw.as_ptr(), &mut pid, &mut uid, &mut gid) };
        ClientCredentials { pid, uid, gid }
    }

    pub fn identity(self) -> usize {
        self.raw.as_ptr() as usize
    }

    /// Creates a resource owned by this client.
    ///
    /// # Safety
    ///
    /// `interface` and every descriptor/string/type pointer reachable from it must remain valid
    /// until the resource is destroyed. The client must still be live.
    pub unsafe fn create_resource(
        self,
        interface: &ffi::wl_interface,
        version: u32,
        id: u32,
    ) -> ServerResult<ResourceRef<'callback>> {
        if version == 0 || version > interface.version as u32 || version > c_int::MAX as u32 {
            return Err(WaylandServerError::new(
                WaylandServerErrorKind::InvalidVersion,
                "Wayland resource version is outside the interface contract",
            ));
        }
        let raw =
            unsafe { ffi::wl_resource_create(self.raw.as_ptr(), interface, version as c_int, id) };
        unsafe { ResourceRef::from_raw(raw) }.ok_or_else(|| {
            WaylandServerError::new(
                WaylandServerErrorKind::Allocation,
                "libwayland could not allocate a resource",
            )
        })
    }

    pub fn object(self, id: u32) -> Option<ResourceRef<'callback>> {
        let raw = unsafe { ffi::wl_client_get_object(self.raw.as_ptr(), id) };
        unsafe { ResourceRef::from_raw(raw) }
    }

    pub fn flush(self) {
        unsafe { ffi::wl_client_flush(self.raw.as_ptr()) };
    }

    pub fn post_no_memory(self) {
        unsafe { ffi::wl_client_post_no_memory(self.raw.as_ptr()) };
    }

    pub fn disconnect(self) {
        unsafe { ffi::wl_client_destroy(self.raw.as_ptr()) };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClientCredentials {
    pub pid: i32,
    pub uid: u32,
    pub gid: u32,
}

#[derive(Clone, Copy)]
pub struct ResourceRef<'callback> {
    raw: NonNull<ffi::wl_resource>,
    marker: PhantomData<&'callback mut ffi::wl_resource>,
}

impl<'callback> ResourceRef<'callback> {
    /// Borrows a resource pointer supplied by a live libwayland callback or lookup.
    ///
    /// # Safety
    ///
    /// `raw` must either be null or identify a live `wl_resource` for the entire `'callback`
    /// lifetime. The caller must prevent destruction while a returned reference is in use.
    pub unsafe fn from_raw(raw: *mut ffi::wl_resource) -> Option<Self> {
        NonNull::new(raw).map(|raw| Self {
            raw,
            marker: PhantomData,
        })
    }

    pub fn id(self) -> u32 {
        unsafe { ffi::wl_resource_get_id(self.raw.as_ptr()) }
    }

    pub fn identity(self) -> usize {
        self.raw.as_ptr() as usize
    }

    pub fn version(self) -> u32 {
        unsafe { ffi::wl_resource_get_version(self.raw.as_ptr()) as u32 }
    }

    pub fn client(self) -> ClientRef<'callback> {
        let raw = unsafe { ffi::wl_resource_get_client(self.raw.as_ptr()) };
        unsafe { ClientRef::from_raw(raw) }.expect("a live Wayland resource always has a client")
    }

    pub fn user_data(self) -> *mut c_void {
        unsafe { ffi::wl_resource_get_user_data(self.raw.as_ptr()) }
    }

    /// Replaces the opaque callback data associated with this resource.
    ///
    /// # Safety
    ///
    /// `data` must remain valid for every native access until it is replaced or the resource is
    /// destroyed.
    pub unsafe fn set_user_data(self, data: *mut c_void) {
        unsafe { ffi::wl_resource_set_user_data(self.raw.as_ptr(), data) };
    }

    /// Installs the native request dispatcher and destruction callback for this resource.
    ///
    /// # Safety
    ///
    /// The function pointers must obey the libwayland ABI and must not unwind. `implementation`
    /// and `data` must remain valid until destruction, and `destroy` must release them exactly once
    /// when ownership requires it.
    pub unsafe fn set_dispatcher(
        self,
        dispatcher: ffi::wl_dispatcher_func_t,
        implementation: *const c_void,
        data: *mut c_void,
        destroy: ffi::wl_resource_destroy_func_t,
    ) {
        unsafe {
            ffi::wl_resource_set_dispatcher(
                self.raw.as_ptr(),
                dispatcher,
                implementation,
                data,
                destroy,
            )
        };
    }

    pub fn post_error(self, code: u32, message: &str) {
        let message = CString::new(message).unwrap_or_else(|_| {
            CString::new("Telorgon rejected a malformed Wayland request").unwrap()
        });
        const STRING_FORMAT: &[u8] = b"%s\0";
        unsafe {
            ffi::wl_resource_post_error(
                self.raw.as_ptr(),
                code,
                STRING_FORMAT.as_ptr().cast(),
                message.as_ptr(),
            )
        };
    }

    /// Posts one protocol event using the argument layout for `opcode`.
    ///
    /// # Safety
    ///
    /// `opcode` must select an event on this resource's negotiated interface and `arguments` must
    /// exactly match its native signature. Every pointer/FD referenced by the argument union must
    /// satisfy libwayland's lifetime and ownership rules for the duration of the call.
    pub unsafe fn post_event(self, opcode: u32, arguments: &mut [ffi::wl_argument]) {
        unsafe {
            ffi::wl_resource_post_event_array(self.raw.as_ptr(), opcode, arguments.as_mut_ptr())
        };
    }

    pub fn post_no_memory(self) {
        unsafe { ffi::wl_resource_post_no_memory(self.raw.as_ptr()) };
    }

    /// Destroys this resource through libwayland.
    ///
    /// # Safety
    ///
    /// No further use may be made of this value or any alias after the call. The resource must not
    /// already have been destroyed by client teardown or a reentrant callback.
    pub unsafe fn destroy(self) {
        unsafe { ffi::wl_resource_destroy(self.raw.as_ptr()) };
    }
}

fn native_zero(
    result: c_int,
    kind: WaylandServerErrorKind,
    message: &'static str,
) -> ServerResult<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(WaylandServerError::new(kind, message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_conversion_rejects_values_beyond_native_range_without_dispatching() {
        let oversized = Duration::from_millis(i32::MAX as u64 + 1);
        assert!(i32::try_from(oversized.as_millis()).is_err());
    }

    #[test]
    fn wrapper_types_remain_owner_thread_values() {
        fn assert_not_copy<T>() {}
        assert_not_copy::<Display>();
        assert_not_copy::<EventSource>();
    }
}
