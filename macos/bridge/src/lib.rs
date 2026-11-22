//! The shared session behind a C interface, for the Mac app's Swift.
//!
//! `ohc_bridge.h` is the interface and says who calls what from where. This
//! file implements the three `platform` traits by calling the functions
//! Swift supplied, hands each snapshot over as JSON, and reads commands from
//! JSON.

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Arc, Mutex, OnceLock};

use ohc_core::models::generation_from_name;
use ohc_core::platform::{Bluetooth, Headset, Incoming, Link};
use ohc_core::protocol::framing::Reassembler;
use ohc_core::protocol::Generation;
use ohc_core::session::{self, Command, Handle, Snapshot};

/// Mirrors `OhcBluetooth` in `ohc_bridge.h`, field for field.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OhcBluetooth {
    radio_is_on: extern "C" fn() -> bool,
    find_headset: extern "C" fn() -> *mut c_void,
    headset_name: extern "C" fn(*mut c_void) -> *mut c_char,
    headset_is_connected: extern "C" fn(*mut c_void) -> bool,
    headset_open: extern "C" fn(*mut c_void, *const c_void, *mut *mut c_void, *mut u8) -> *mut c_char,
    headset_connect_audio: extern "C" fn(*mut c_void) -> *mut c_char,
    headset_disconnect_audio: extern "C" fn(*mut c_void) -> *mut c_char,
    headset_release: extern "C" fn(*mut c_void),
    link_send: extern "C" fn(*mut c_void, *const u8, usize) -> *mut c_char,
    link_close: extern "C" fn(*mut c_void),
    on_snapshot: extern "C" fn(*const c_char),
}

static SESSION: OnceLock<Handle> = OnceLock::new();

extern "C" {
    fn free(pointer: *mut c_void);
}

/// Takes over a string Swift made with `strdup`. `None` for NULL.
fn take(text: *mut c_char) -> Option<String> {
    if text.is_null() {
        return None;
    }
    // SAFETY: the header promises a NUL-terminated string from `strdup`.
    unsafe {
        let owned = CStr::from_ptr(text).to_string_lossy().into_owned();
        free(text.cast());
        Some(owned)
    }
}

/// For the functions that return an error message, or NULL when it worked.
fn done(error: *mut c_char) -> Result<(), String> {
    take(error).map_or(Ok(()), Err)
}

struct Mac(OhcBluetooth);

impl Bluetooth for Mac {
    fn radio_is_on(&self) -> bool {
        (self.0.radio_is_on)()
    }

    fn find_headset(&self) -> Option<Box<dyn Headset>> {
        let handle = (self.0.find_headset)();
        if handle.is_null() {
            return None;
        }
        let name = take((self.0.headset_name)(handle)).unwrap_or_default();
        Some(Box::new(MacHeadset { swift: self.0, handle, name }))
    }
}

struct MacHeadset {
    swift: OhcBluetooth,
    handle: *mut c_void,
    name: String,
}

impl Headset for MacHeadset {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_connected(&self) -> bool {
        (self.swift.headset_is_connected)(self.handle)
    }

    fn open(&self) -> Result<(Box<dyn Link>, Generation), String> {
        let inbox = Arc::new(Mutex::new(Inbox::default()));
        let mut handle = std::ptr::null_mut();
        let mut generation = 0u8;
        done((self.swift.headset_open)(self.handle, Arc::as_ptr(&inbox).cast(), &mut handle, &mut generation))?;
        // From here on dropping the link closes the channel.
        let link = MacLink { swift: self.swift, handle, inbox };
        let generation = match generation {
            1 => Generation::V1,
            2 => Generation::V2,
            other => return Err(format!("The control channel speaks an unknown generation ({other})")),
        };
        Ok((Box::new(link), generation))
    }

    fn connect_audio(&self) -> Result<(), String> {
        done((self.swift.headset_connect_audio)(self.handle))
    }

    fn disconnect_audio(&self) -> Result<(), String> {
        done((self.swift.headset_disconnect_audio)(self.handle))
    }
}

impl Drop for MacHeadset {
    fn drop(&mut self) {
        (self.swift.headset_release)(self.handle);
    }
}

type Listener = Box<dyn FnMut(Incoming) + Send>;

/// Where a channel's bytes become frames. Swift holds a pointer to it from
/// `headset_open` until `link_close` returns, which is why `MacLink` keeps
/// it alive until then.
#[derive(Default)]
struct Inbox {
    reassembler: Reassembler,
    listener: Option<Listener>,
    /// What arrived before the session started listening.
    early: Vec<Incoming>,
}

impl Inbox {
    fn deliver(&mut self, incoming: Incoming) {
        match &mut self.listener {
            Some(listener) => listener(incoming),
            None => self.early.push(incoming),
        }
    }
}

struct MacLink {
    swift: OhcBluetooth,
    handle: *mut c_void,
    inbox: Arc<Mutex<Inbox>>,
}

impl Link for MacLink {
    fn listen(&self, mut on_incoming: Listener) {
        let mut inbox = self.inbox.lock().unwrap();
        inbox.early.drain(..).for_each(&mut on_incoming);
        inbox.listener = Some(on_incoming);
    }

    fn send(&self, bytes: &[u8]) -> Result<(), String> {
        done((self.swift.link_send)(self.handle, bytes.as_ptr(), bytes.len()))
    }
}

impl Drop for MacLink {
    fn drop(&mut self) {
        (self.swift.link_close)(self.handle);
    }
}

/// Hands each snapshot to Swift as JSON.
fn report(on_snapshot: extern "C" fn(*const c_char)) -> impl Fn(&Snapshot) + Send + 'static {
    move |snapshot| {
        let json = serde_json::to_string(snapshot).expect("a snapshot always serialises");
        let json = CString::new(json).expect("JSON has no NUL in it");
        on_snapshot(json.as_ptr());
    }
}

/// # Safety
/// `bluetooth` points to a filled-in `OhcBluetooth`.
#[no_mangle]
pub unsafe extern "C" fn ohc_start(bluetooth: *const OhcBluetooth) {
    let swift = *bluetooth;
    SESSION.get_or_init(|| {
        // The app's standard error; `OHC_TRACE=1` adds every message.
        ohc_core::logger::set_sink(|line| eprintln!("{line}"));
        session::spawn(Mac(swift), report(swift.on_snapshot))
    });
}

/// # Safety
/// `model` is a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn ohc_start_demo(model: *const c_char, on_snapshot: extern "C" fn(*const c_char)) {
    let model = CStr::from_ptr(model).to_string_lossy().into_owned();
    SESSION.get_or_init(|| session::spawn_demo(model, report(on_snapshot)));
}

/// # Safety
/// `command_json` is a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn ohc_send(command_json: *const c_char) -> bool {
    let json = CStr::from_ptr(command_json).to_string_lossy();
    match (SESSION.get(), serde_json::from_str::<Command>(&json)) {
        (Some(session), Ok(command)) => {
            session.send(command);
            true
        }
        _ => false,
    }
}

/// # Safety
/// `name` is a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn ohc_is_headset_name(name: *const c_char) -> bool {
    generation_from_name(&CStr::from_ptr(name).to_string_lossy()).is_some()
}

/// # Safety
/// `inbox` is the one `headset_open` was given and `link_close` has not
/// returned yet; `bytes` points to `length` bytes.
#[no_mangle]
pub unsafe extern "C" fn ohc_link_received(inbox: *const c_void, bytes: *const u8, length: usize) {
    let mut inbox = (*inbox.cast::<Mutex<Inbox>>()).lock().unwrap();
    for frame in inbox.reassembler.feed(std::slice::from_raw_parts(bytes, length)) {
        inbox.deliver(Incoming::Frame(frame));
    }
}

/// # Safety
/// As for [`ohc_link_received`]; `reason` is NULL or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn ohc_link_closed(inbox: *const c_void, reason: *const c_char) {
    let reason = (!reason.is_null()).then(|| CStr::from_ptr(reason).to_string_lossy().into_owned());
    (*inbox.cast::<Mutex<Inbox>>()).lock().unwrap().deliver(Incoming::Closed(reason));
}
