//! What every Open Headphones Connect app shares: Sony's control protocol
//! and the session that speaks it.
//!
//! Nothing here touches an operating system. The app that hosts this crate
//! supplies Bluetooth through the traits in [`platform`] and gets back a
//! [`session::Snapshot`] every time something about the headset changes.

pub mod logger;
pub mod models;
pub mod platform;
pub mod protocol;
pub mod session;
