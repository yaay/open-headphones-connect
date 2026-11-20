//! The RFCOMM socket to the headset's control service.
//!
//! This uses a plain Bluetooth socket (as Plutoberth's client does) rather
//! than the WinRT RFCOMM API: the WH-1000XM3 keeps its control service out of
//! the public service list, so WinRT cannot see it, while a socket connect
//! asks the headset for that one service by its id.

use std::sync::Arc;
use std::sync::Once;
use std::thread;

use windows::core::GUID;
use windows::Win32::Devices::Bluetooth::{AF_BTH, BTHPROTO_RFCOMM, SOCKADDR_BTH};
use windows::Win32::Networking::WinSock::{
    closesocket, connect, recv, send, shutdown, socket, WSAGetLastError, WSAStartup, SD_BOTH,
    SEND_RECV_FLAGS, SOCKADDR, SOCKET, SOCKET_ERROR, SOCK_STREAM, WSADATA,
};

use super::{service_uuid, SonyDevice};
use crate::protocol::framing::{Frame, Reassembler};
use crate::protocol::Generation;

pub enum Incoming {
    Frame(Frame),
    /// The socket ended; carries the reason when it was not a clean close.
    Closed(Option<String>),
}

/// Closes the socket once the link and its reader thread have both let go.
struct Socket(SOCKET);

impl Drop for Socket {
    fn drop(&mut self) {
        unsafe {
            closesocket(self.0);
        }
    }
}

// A socket handle may be used from any thread; reading and writing at the
// same time from two threads is supported.
unsafe impl Send for Socket {}
unsafe impl Sync for Socket {}

pub struct Link {
    socket: Arc<Socket>,
}

fn last_error(action: &str) -> String {
    let code = unsafe { WSAGetLastError() }.0;
    let meaning = match code {
        10048 => "the control channel is already in use",
        10049 => "the headset does not offer this service",
        10050 | 10051 | 10065 => "the headset is out of reach",
        10053 | 10054 => "the headset closed the connection",
        10060 => "the headset did not answer in time",
        10064 => "the headset is switched off or out of range",
        _ => "Bluetooth error",
    };
    format!("{action}: {meaning} (code {code})")
}

fn start_winsock() {
    static START: Once = Once::new();
    START.call_once(|| unsafe {
        let mut data = WSADATA::default();
        WSAStartup(0x0202, &mut data);
    });
}

impl Link {
    /// Opens the control channel, trying the generation the name suggests
    /// first and the other one as a fallback.
    pub fn open(device: &SonyDevice) -> Result<(Link, Generation), String> {
        let other = match device.generation {
            Generation::V1 => Generation::V2,
            Generation::V2 => Generation::V1,
        };
        let first = match Self::open_service(device.address, service_uuid(device.generation)) {
            Ok(link) => return Ok((link, device.generation)),
            Err(e) => e,
        };
        match Self::open_service(device.address, service_uuid(other)) {
            Ok(link) => Ok((link, other)),
            // The first failure is the more telling one.
            Err(_) => Err(first),
        }
    }

    fn open_service(address: u64, service: GUID) -> Result<Link, String> {
        start_winsock();
        unsafe {
            let handle = socket(AF_BTH as i32, SOCK_STREAM, BTHPROTO_RFCOMM as i32)
                .map_err(|e| format!("Could not create a Bluetooth socket: {}", e.message()))?;
            let socket = Arc::new(Socket(handle));
            let target = SOCKADDR_BTH {
                addressFamily: AF_BTH,
                btAddr: address,
                serviceClassId: service,
                port: 0,
            };
            let result = connect(
                handle,
                &target as *const SOCKADDR_BTH as *const SOCKADDR,
                std::mem::size_of::<SOCKADDR_BTH>() as i32,
            );
            if result == SOCKET_ERROR {
                return Err(last_error("Connecting to the control service failed"));
            }
            Ok(Link { socket })
        }
    }

    /// Reads on a background thread until the socket closes.
    pub fn spawn_reader(&self, mut on_incoming: impl FnMut(Incoming) + Send + 'static) {
        let socket = self.socket.clone();
        thread::spawn(move || {
            let mut reassembler = Reassembler::default();
            let mut buffer = [0u8; 2048];
            let reason = loop {
                let count = unsafe { recv(socket.0, &mut buffer, SEND_RECV_FLAGS(0)) };
                if count == 0 {
                    break None;
                }
                if count < 0 {
                    break Some(last_error("Reading from the headset failed"));
                }
                for frame in reassembler.feed(&buffer[..count as usize]) {
                    on_incoming(Incoming::Frame(frame));
                }
            };
            on_incoming(Incoming::Closed(reason));
        });
    }

    pub fn send(&self, bytes: &[u8]) -> Result<(), String> {
        let mut sent = 0;
        while sent < bytes.len() {
            let count = unsafe { send(self.socket.0, &bytes[sent..], SEND_RECV_FLAGS(0)) };
            if count <= 0 {
                return Err(last_error("Sending to the headset failed"));
            }
            sent += count as usize;
        }
        Ok(())
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        // Unblocks the reader thread, which then releases the socket.
        unsafe {
            shutdown(self.socket.0, SD_BOTH);
        }
    }
}
