//! What the session needs from the operating system's Bluetooth.
//!
//! Each app implements these three traits: the Windows app over WinRT and a
//! Bluetooth socket, the Mac app over IOBluetooth. Everything is called from
//! the session's own thread, apart from the closure given to
//! [`Link::listen`], which the implementation may call from any one thread.

use crate::protocol::framing::Frame;
use crate::protocol::Generation;

/// The computer's Bluetooth.
pub trait Bluetooth: Send {
    /// Runs once on the session thread before anything else is called.
    fn init_thread(&self) {}

    fn radio_is_on(&self) -> bool;

    /// Looks through the paired devices for a Sony headset. One that is
    /// connected right now wins over one that is merely paired.
    fn find_headset(&self) -> Option<Box<dyn Headset>>;
}

/// A paired Sony headset.
pub trait Headset {
    fn name(&self) -> &str;

    /// Whether the headset is connected to this computer right now. The
    /// session asks about once a second, so this has to be cheap.
    fn is_connected(&self) -> bool;

    /// Opens the control channel and says which generation of the protocol
    /// is on the other end.
    fn open(&self) -> Result<(Box<dyn Link>, Generation), String>;

    /// Asks the system to connect the headset. Returns once the request is
    /// made; the session then watches [`Headset::is_connected`]. An error's
    /// message is shown to the user.
    fn connect_audio(&self) -> Result<(), String>;

    fn disconnect_audio(&self) -> Result<(), String>;
}

/// What arrives on a control channel.
pub enum Incoming {
    /// A whole Sony frame, from a platform that reassembles them itself.
    Frame(Frame),
    /// Bytes as they came off the channel; the session puts frames together from them, in
    /// whichever protocol the channel speaks.
    Bytes(Vec<u8>),
    /// The channel ended; carries the reason when it was not a clean close.
    Closed(Option<String>),
}

/// An open control channel. Dropping it closes the channel.
pub trait Link {
    /// Starts delivering what arrives, in order, ending with one
    /// [`Incoming::Closed`]. Called once, right after the channel opens.
    /// [`crate::protocol::framing::Reassembler`] turns received bytes into
    /// frames.
    fn listen(&self, on_incoming: Box<dyn FnMut(Incoming) + Send>);

    fn send(&self, bytes: &[u8]) -> Result<(), String>;
}
