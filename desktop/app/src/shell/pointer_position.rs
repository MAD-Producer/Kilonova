//! Native per-window coordinates, separate from the widget's invalid outside hit point.
use motion_ui::input::Event;
use winit::dpi::PhysicalPosition;

#[derive(Default)]
pub(crate) struct PointerPosition(Option<PhysicalPosition<f64>>);

impl PointerPosition {
    pub(crate) fn moved(&mut self, position: PhysicalPosition<f64>, scale: f32) -> Event {
        self.0 = Some(position);
        Self::motion(position, scale)
    }

    // winit 0.30's Windows WM_MOUSEMOVE handler emits CursorEntered but suppresses
    // CursorMoved when the client coordinate equals its last cached position.
    // Restore that same coordinate on entry; an ensuing actual move replaces it.
    // CursorLeft still invalidates widget hits and cancels the previous gesture.
    #[cfg(target_os = "windows")]
    pub(crate) fn entered(&self, scale: f32) -> Option<Event> {
        self.0.map(|position| Self::motion(position, scale))
    }

    fn motion(position: PhysicalPosition<f64>, scale: f32) -> Event {
        Event::MouseMoved {
            position: [position.x as f32 / scale, position.y as f32 / scale],
        }
    }
}

// Added to shell/pointer_position.rs; uses the receiving window's client space.
impl PointerPosition {
    pub(crate) fn dropped(
        &mut self,
        window: &winit::window::Window,
        scale: f32,
    ) -> std::io::Result<[f32; 2]> {
        if !scale.is_finite() || scale <= 0. {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid file-drop scale",
            ));
        }
        #[cfg(target_os = "windows")]
        let position = {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let handle = window.window_handle().map_err(std::io::Error::other)?;
            let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "file-drop window has no Win32 handle",
                ));
            };
            native_drop_point(handle.hwnd.get())?
        };
        #[cfg(target_os = "macos")]
        let position = native_macos_drop_point(window, scale)?;
        #[cfg(target_os = "linux")]
        let position = native_x11_drop_point(window)?;
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        let position = {
            let _ = window;
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "native file-drop coordinates are unavailable on this platform",
            ));
        };
        let logical = [position.x as f32 / scale, position.y as f32 / scale];
        if !logical.iter().all(|v| v.is_finite()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "non-finite file-drop coordinate",
            ));
        }
        self.0 = Some(position);
        Ok(logical)
    }
}

#[cfg(target_os = "windows")]
fn native_drop_point(window: isize) -> std::io::Result<PhysicalPosition<f64>> {
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetCursorPos(point: *mut Point) -> i32;
        fn ScreenToClient(window: *mut std::ffi::c_void, point: *mut Point) -> i32;
    }
    // OLE drag events do not update winit's cached CursorMoved coordinate.
    // Read at receipt and project through this exact native client handle.
    let mut point = Point { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { ScreenToClient(window as *mut _, &mut point) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(PhysicalPosition::new(point.x as f64, point.y as f64))
}

#[cfg(target_os = "macos")]
fn native_macos_drop_point(
    window: &winit::window::Window,
    scale: f32,
) -> std::io::Result<PhysicalPosition<f64>> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ffi::{c_char, c_void};
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[link(name = "objc")]
    unsafe extern "C" {
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
    }
    let handle = window.window_handle().map_err(std::io::Error::other)?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(std::io::Error::other("file-drop window has no AppKit view"));
    };
    // Called on winit's owner thread. WinitView is flipped (upper-left origin).
    // NSWindow reports base coordinates; convert through this exact NSView.
    unsafe {
        let send_pointer: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let send_point: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Point =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let convert: unsafe extern "C" fn(*mut c_void, *mut c_void, Point, *mut c_void) -> Point =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let view = handle.ns_view.as_ptr();
        let native = send_pointer(view, sel_registerName(c"window".as_ptr()));
        if native.is_null() {
            return Err(std::io::Error::other("file-drop AppKit view is detached"));
        }
        let point = send_point(
            native,
            sel_registerName(c"mouseLocationOutsideOfEventStream".as_ptr()),
        );
        let point = convert(
            view,
            sel_registerName(c"convertPoint:fromView:".as_ptr()),
            point,
            std::ptr::null_mut(),
        );
        Ok(PhysicalPosition::new(
            point.x * f64::from(scale),
            point.y * f64::from(scale),
        ))
    }
}

#[cfg(target_os = "linux")]
fn native_x11_drop_point(window: &winit::window::Window) -> std::io::Result<PhysicalPosition<f64>> {
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void};
    let display = window.display_handle().map_err(std::io::Error::other)?;
    let handle = window.window_handle().map_err(std::io::Error::other)?;
    let (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(window)) =
        (display.as_raw(), handle.as_raw())
    else {
        // winit 0.30 does not deliver file-drop events on Wayland. Do not use
        // an old pointer cache if another backend starts delivering them.
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "native file-drop coordinates require an Xlib window",
        ));
    };
    let display = display
        .display
        .ok_or_else(|| std::io::Error::other("Xlib file-drop display missing"))?;
    #[link(name = "dl")]
    unsafe extern "C" {
        fn dlopen(name: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(library: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dlclose(library: *mut c_void) -> c_int;
        fn dlerror() -> *const c_char;
    }
    fn loader_cause() -> String {
        let cause = unsafe { dlerror() };
        if cause.is_null() {
            "dynamic loader did not provide a cause".into()
        } else {
            unsafe { std::ffi::CStr::from_ptr(cause) }
                .to_string_lossy()
                .chars()
                .take(512)
                .collect()
        }
    }
    struct Library(*mut c_void);
    impl Drop for Library {
        fn drop(&mut self) {
            if unsafe { dlclose(self.0) } != 0 {
                log::error!(
                    "node image drop phase=close_x11_query_library: {}",
                    loader_cause()
                );
            }
        }
    }
    let library = unsafe { dlopen(c"libX11.so.6".as_ptr(), 1) };
    if library.is_null() {
        return Err(std::io::Error::other(format!(
            "cannot load Xlib file-drop pointer query: {}",
            loader_cause()
        )));
    }
    let library = Library(library);
    let query = unsafe { dlsym(library.0, c"XQueryPointer".as_ptr()) };
    if query.is_null() {
        return Err(std::io::Error::other(format!(
            "XQueryPointer is unavailable: {}",
            loader_cause()
        )));
    }
    let query: unsafe extern "C" fn(
        *mut c_void,
        c_ulong,
        *mut c_ulong,
        *mut c_ulong,
        *mut c_int,
        *mut c_int,
        *mut c_int,
        *mut c_int,
        *mut c_uint,
    ) -> c_int = unsafe { std::mem::transmute(query) };
    let (mut root, mut child, mut root_x, mut root_y, mut x, mut y, mut mask) =
        (0, 0, 0, 0, 0, 0, 0);
    if unsafe {
        query(
            display.as_ptr(),
            window.window,
            &mut root,
            &mut child,
            &mut root_x,
            &mut root_y,
            &mut x,
            &mut y,
            &mut mask,
        )
    } == 0
    {
        return Err(std::io::Error::other(
            "file-drop pointer is on a different X11 screen",
        ));
    }
    Ok(PhysicalPosition::new(f64::from(x), f64::from(y)))
}
