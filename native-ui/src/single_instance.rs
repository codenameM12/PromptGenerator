//! Single-instance detection with a focus request channel.
//!
//! The first instance binds a loopback port and keeps it for the process
//! lifetime. A second launch fails to bind, sends a one-byte focus request to
//! the first process, and exits without opening a window or a database
//! connection. The first instance polls the listener from its UI loop and
//! issues `ViewportCommand::Focus` when a request arrives.
//!
//! LIMITATION: this proves "no second window/connection" deterministically, but
//! bringing an existing window to the foreground is a best-effort focus command
//! that cannot be verified in a headless session. See the final report.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

pub const DEFAULT_PORT: u16 = 49_173;

#[cfg(windows)]
fn grant_foreground_rights() {
    const ASFW_ANY: u32 = u32::MAX;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn AllowSetForegroundWindow(process_id: u32) -> i32;
    }
    // SAFETY: ASFW_ANY is the documented value that lets any process take the
    // foreground; the call has no pointers or lifetime requirements.
    unsafe {
        AllowSetForegroundWindow(ASFW_ANY);
    }
}

#[cfg(not(windows))]
fn grant_foreground_rights() {}

pub enum Acquisition {
    First(SingleInstance),
    AlreadyRunning,
}

pub struct SingleInstance {
    listener: TcpListener,
    focus_requested: bool,
}

impl SingleInstance {
    pub fn acquire(port: u16) -> Acquisition {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(listener) => {
                let _ = listener.set_nonblocking(true);
                Acquisition::First(Self {
                    listener,
                    focus_requested: false,
                })
            }
            Err(_) => {
                // Windows only lets the foreground process raise another one.
                // The user just launched us from the shell, so hand those rights
                // to the running instance before signalling it.
                grant_foreground_rights();
                // Another instance owns the port; ask it to focus itself.
                if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
                    let _ = stream.write_all(&[1]);
                    let _ = stream.flush();
                }
                Acquisition::AlreadyRunning
            }
        }
    }

    /// Drains pending focus requests. Returns true if at least one arrived.
    pub fn poll(&mut self) -> bool {
        loop {
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    // Accepted sockets can inherit the listener's non-blocking
                    // mode, so the byte may not have landed yet; wait briefly.
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(200)));
                    let mut byte = [0u8; 1];
                    if stream.read(&mut byte).map(|n| n > 0).unwrap_or(false) {
                        self.focus_requested = true;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        self.take_focus_request()
    }

    pub fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.focus_requested)
    }
}

#[cfg(test)]
mod tests {
    use super::{Acquisition, SingleInstance};

    fn test_port() -> u16 {
        45_000 + (std::process::id() % 5_000) as u16
    }

    #[test]
    fn second_launch_is_rejected_and_signals_focus() {
        let port = test_port();
        let mut first = match SingleInstance::acquire(port) {
            Acquisition::First(instance) => instance,
            Acquisition::AlreadyRunning => return, // port busy; skip
        };
        match SingleInstance::acquire(port) {
            Acquisition::AlreadyRunning => {}
            Acquisition::First(_) => panic!("second launch must not acquire the port"),
        }
        assert!(
            first.poll(),
            "first instance should observe the focus request"
        );
        assert!(!first.take_focus_request(), "request is consumed once");
    }
}
