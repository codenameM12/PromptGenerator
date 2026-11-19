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
