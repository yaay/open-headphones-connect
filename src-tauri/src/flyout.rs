//! Places the single window as a Windows 11 style flyout: bottom-right, just
//! above the taskbar, sliding up as it appears.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetForegroundWindow, IsWindowVisible, ShowWindow, SystemParametersInfoW,
    GA_ROOTOWNER, SPI_GETWORKAREA, SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

pub const WIDTH: f64 = 360.0;
const MARGIN: f64 = 12.0;
const SLIDE_DISTANCE: f64 = 40.0;
const SLIDE_TIME: Duration = Duration::from_millis(260);

/// Cancels a running slide when a newer placement starts.
#[derive(Clone, Default)]
pub struct Motion(Arc<AtomicU64>);

fn hwnd(window: &WebviewWindow) -> Option<HWND> {
    // Go through the raw pointer so this does not depend on which version of
    // the `windows` crate Tauri itself was built against.
    window.hwnd().ok().map(|h| HWND(h.0 as *mut c_void))
}

/// The primary monitor's desktop area, excluding the taskbar, in pixels.
fn work_area() -> Option<RECT> {
    let mut rect = RECT::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rect as *mut RECT as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .ok()?;
    }
    Some(rect)
}

pub struct Placement {
    pub height: f64,
    /// Slide up from below instead of appearing in place.
    pub slide: bool,
    /// Take keyboard focus. The connect card must not steal it.
    pub activate: bool,
}

pub fn place(window: &WebviewWindow, motion: &Motion, placement: Placement) {
    let Some(area) = work_area() else { return };
    let scale = window.scale_factor().unwrap_or(1.0);
    let width = (WIDTH * scale).round() as i32;
    let margin = (MARGIN * scale).round() as i32;
    // Never taller than the screen has room for; the page scrolls instead.
    let room = (area.bottom - area.top - 2 * margin).max(1);
    let height = ((placement.height * scale).round() as i32).min(room);
    let ticket = motion.0.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = window.set_size(PhysicalSize::new(width as u32, height as u32));

    // Windows keeps an invisible resize border around the window, so the
    // position it takes is not where the visible content starts.
    let (inset_x, inset_y) = match (window.inner_position(), window.outer_position()) {
        (Ok(inner), Ok(outer)) => (inner.x - outer.x, inner.y - outer.y),
        _ => (0, 0),
    };
    let x = area.right - width - margin - inset_x;
    let y = area.bottom - height - margin - inset_y;

    let start_y = if placement.slide { y + (SLIDE_DISTANCE * scale).round() as i32 } else { y };
    let _ = window.set_position(PhysicalPosition::new(x, start_y));

    if placement.activate {
        let _ = window.show();
        // The same caution as in `hide`: make sure it really is showing.
        if let Some(handle) = hwnd(window) {
            unsafe {
                let _ = ShowWindow(handle, SW_SHOW);
            }
        }
        let _ = window.set_focus();
    } else if let Some(handle) = hwnd(window) {
        unsafe {
            let _ = ShowWindow(handle, SW_SHOWNOACTIVATE);
        }
    }

    if !placement.slide {
        return;
    }
    let window = window.clone();
    let motion = motion.clone();
    thread::spawn(move || {
        let started = Instant::now();
        loop {
            if motion.0.load(Ordering::SeqCst) != ticket {
                return;
            }
            let t = (started.elapsed().as_secs_f64() / SLIDE_TIME.as_secs_f64()).min(1.0);
            // Fluent's "fast invoke" curve is close to an ease-out quint.
            let eased = 1.0 - (1.0 - t).powi(5);
            let now_y = start_y as f64 + (y - start_y) as f64 * eased;
            let _ = window.set_position(PhysicalPosition::new(x, now_y.round() as i32));
            if t >= 1.0 {
                return;
            }
            thread::sleep(Duration::from_millis(7));
        }
    });
}

pub fn hide(window: &WebviewWindow, motion: &Motion) {
    motion.0.fetch_add(1, Ordering::SeqCst);
    // The framework keeps its own note of whether the window is showing and
    // skips a hide it believes is unnecessary. The connect card is shown
    // behind its back (see `place`), so hide the window directly as well.
    let _ = window.hide();
    if let Some(handle) = hwnd(window) {
        unsafe {
            let _ = ShowWindow(handle, SW_HIDE);
        }
    }
}

/// Whether the window is really on screen, asked of Windows itself.
pub fn is_on_screen(window: &WebviewWindow) -> bool {
    hwnd(window).is_some_and(|handle| unsafe { IsWindowVisible(handle).as_bool() })
}

/// True while this window, or a popup it owns such as an open drop-down,
/// is the foreground window.
pub fn has_foreground(window: &WebviewWindow) -> bool {
    let Some(own) = hwnd(window) else { return false };
    unsafe {
        let foreground = GetForegroundWindow();
        !foreground.is_invalid() && (foreground == own || GetAncestor(foreground, GA_ROOTOWNER) == own)
    }
}
