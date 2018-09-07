//! An implementation of a GDB remote stub / debugging server using the GDB
//! remote serial protocol (RSP).
//!
//! This crate provides a basic implementation of a gdbstub, which allows crates
//! to act as debugging proxies for target programs. For example, this can be
//! used in emulators to allow debugging the emulated program.
//!
//! Does not yet handle retransmission. Use a reliable communication channel
//! instead.

#[macro_use] extern crate log;
extern crate byteorder;

mod comm;
mod proto;
pub mod targets;

use comm::*;
pub use comm::Comm;

use proto::{Command, ParseError};
use targets::{EncodeRegister, TargetDesc};

use byteorder::LittleEndian;

use std::{error, mem, str, thread};
use proto::ThreadId;
use proto::ThreadAction;

/// This trait provides an interface between GDB and the target program and must
/// be implemented by the user.
///
/// The GDB stub implementation handles commands from GDB either internally or
/// by calling a method on this trait, which must be implemented by the user of
/// the crate.
pub trait StubCalls {
    /// The target system descriptor.
    type Target: TargetDesc;

    /// Reads the processor's registers.
    fn read_registers(&mut self) -> <Self::Target as TargetDesc>::Registers;

    /// Tries to read a byte from the target system's memory.
    ///
    /// Returns an error if `addr` does not point to valid (mapped) memory.
    fn read_mem(&mut self, addr: u64) -> Result<u8, ()>;

    /// Writes a byte to the target system's memory.
    ///
    /// This is used to manually modify memory and to insert breakpoints.
    ///
    /// Returns an error if `addr` does not point to valid memory. However, if
    /// `addr` is read-only memory, an attempt should be made to modify the
    /// memory anyways (eg. by temporarily remapping the containing page as
    /// writeable).
    fn write_mem(&mut self, addr: u64, byte: u8) -> Result<(), ()>;

    /// Continue running the target program until a signal is received or a
    /// breakpoint is hit.
    fn cont(&mut self);

    /// Kill the target program / system.
    ///
    /// This doesn't need to be implemented. GDB sends this when closing the
    /// connection.
    fn kill(&mut self) {}
}

trait CommExt: Comm {
    fn write_response<F>(&mut self, f: F) -> Result<(), Self::Error>
        where F: FnOnce(&mut ChecksumComm<Self>) -> Result<(), Self::Error>;
}

impl<T: Comm> CommExt for T {
    fn write_response<F>(&mut self, f: F) -> Result<(), Self::Error>
        where F: FnOnce(&mut ChecksumComm<Self>) -> Result<(), Self::Error> {
        self.write(b'$')?;
        let checksum = {
            let mut check = ChecksumComm::new(self);
            f(&mut check)?;
            check.into_checksum()
        };
        self.write(b'#')?;
        self.write_hex(checksum)
    }
}

struct ResponseWriter<'a, C: Comm + 'a> {
    comm: &'a mut C,
    checksum: u8,
    finished: bool,
}

impl<'a, C: Comm> ResponseWriter<'a, C> {
    fn new(comm: &'a mut C) -> Result<Self, Error> {
        comm.write(b'$').map_err(Error::comm)?;
        Ok(Self {
            comm,
            checksum: 0,
            finished: false,
        })
    }

    fn finish(mut self) -> Result<(), Error> {
        self.finished = true;
        self.comm.write(b'#').map_err(Error::comm)?;
        self.comm.write_hex(self.checksum).map_err(Error::comm)
    }
}

impl<'a, C: Comm> Comm for ResponseWriter<'a, C> {
    type Error = C::Error;

    fn read(&mut self) -> Result<u8, C::Error> {
        panic!("attempted to read using ResponseWriter");
    }

    fn write(&mut self, byte: u8) -> Result<(), <Self as Comm>::Error> {
        self.checksum = self.checksum.wrapping_add(byte);
        self.comm.write(byte)
    }
}

impl<'a, C: Comm> Drop for ResponseWriter<'a, C> {
    fn drop(&mut self) {
        if !thread::panicking() {
            assert!(self.finished, "dropped ResponseWriter without calling `finish`");
        }
    }
}

/// A GDB target connected via the remote debugging protocol.
pub struct GdbStub<C: Comm, T: StubCalls> {
    comm: C,
    target: T,
    /// Packet buffer,
    buf: Vec<u8>,
    next: u8,
    /// Active thread for continue and step operations.
    thread_cont_step: ThreadId,
    /// Active thread for other operations.
    thread_other: ThreadId,
}

impl<C: Comm, T: StubCalls> GdbStub<C, T> {
    /// Creates a new `GdbStub` instance.
    pub fn new(comm: C, target: T) -> Self {
        GdbStub {
            comm,
            target,
            buf: Vec::new(),
            next: 0,
            thread_cont_step: ThreadId::All,
            thread_other: ThreadId::Any,
        }
    }

    /// Starts polling for and replying to incoming commands.
    ///
    /// This blocks until the debugger closes the connection.
    // FIXME: Rename? It practically does interactive debugging.
    pub fn poll(mut self) -> Result<(), Error> {
        loop {
            self.next = self.read()?;
            match self.next {
                b'$' => {
                    self.read_packet()?;
                    self.write(b'+')?;  // ACK the transmission

                    let mut buf = mem::replace(&mut self.buf, Vec::new());
                    let result = || -> Result<(), Error> {
                        match Command::parse(&mut buf) {
                            Ok(cmd) => {
                                trace!("{:?}", cmd);
                                self.handle_cmd(cmd)
                            }
                            Err(ParseError::Unsupported) => self.write_response(|_| Ok(())),
                            Err(ParseError::Malformed) => Err(Error::Malformed),
                        }
                    }();
                    self.buf = buf;
                    match result {
                        Err(Error::Killed) => {
                            info!("debugger killed connection");
                            return Ok(());
                        },
                        res => res?,    // Ok => continue
                    }
                },
                b'+' => {}
                b'-' => return Err(Error::Nack),
                _ => return Err(Error::unexpected(self.next, "start of packet ($) or ACK (+)")),
            }
        }
    }

    /// Process a parsed command and send the corresponding response.
    ///
    /// The command packet must already be acknowledged.
    fn handle_cmd(&mut self, cmd: Command) -> Result<(), Error> {
        match cmd {
            Command::GetHaltReason => self.write_response(|c| c.write_all(b"S00")),
            Command::ReadRegisters => {
                let regs = self.target.read_registers();
                self.write_response(|comm| regs.encode::<_, LittleEndian>(comm))
            },
            Command::Kill => {
                self.target.kill();
                Err(Error::Killed)
            }
            Command::SetThread { action, thread } => {
                match action {
                    ThreadAction::ContStep => self.thread_cont_step = thread,
                    ThreadAction::Other => self.thread_other = thread,
                }

                let mut resp = ResponseWriter::new(&mut self.comm)?;
                resp.write_all(b"OK").map_err(Error::comm)?;
                resp.finish()?;
                Ok(())
            }
            Command::Continue => {
                self.target.cont();

                let mut resp = ResponseWriter::new(&mut self.comm)?;
                resp.write_all(b"S05").map_err(Error::comm)?; // 05 is apparently the trap signal
                resp.finish()?;
                Ok(())
            }
            Command::ReadMem { start, len } => {
                trace!("reading {} bytes starting at {:#010X}", len, start);
                let mut resp = ResponseWriter::new(&mut self.comm)?;

                for addr in start..start+len {
                    match self.target.read_mem(addr) {
                        Ok(byte) => resp.write_hex(byte).map_err(Error::comm)?,
                        // cancel on errors and return truncated response
                        Err(_) => break,
                    }
                }

                resp.finish()?;
                Ok(())
            }
            Command::WriteMem { start, bytes } => {
                let mut err = false;
                for (addr, byte) in (start..start+bytes.len() as u64).zip(bytes) {
                    match self.target.write_mem(addr, *byte) {
                        Ok(()) => {},
                        Err(_) => {
                            err = true;
                            break;
                        },
                    }
                }

                let mut resp = ResponseWriter::new(&mut self.comm)?;
                if err {
                    // couldn't write all bytes
                    resp.write_all(b"E00").map_err(Error::comm)?;
                } else {
                    resp.write_all(b"OK").map_err(Error::comm)?;
                }
                resp.finish()?;

                Ok(())
            }
        }
    }

    /// Reads a packet into `self.buf`.
    ///
    /// The start of the packet ($-symbol) must already be consumed (and in
    /// `self.next`).
    fn read_packet(&mut self) -> Result<(), Error> {
        self.buf.clear();

        let mut computed_checksum = 0u8;
        loop {
            let b = self.read()?;
            if b == b'#' {
                break;
            }

            self.buf.push(b);
            computed_checksum = computed_checksum.wrapping_add(b);
        }

        let mut checksum = [0u8, 0];
        let checksum = self.read_str(&mut checksum)?;
        trace!("${}#{}", String::from_utf8_lossy(&self.buf), checksum);
        let checksum = u8::from_str_radix(checksum, 16)
            .map_err(|_| Error::unexpected(checksum.as_bytes()[0] /* FIXME */, "checksum (hex byte)"))?;

        if computed_checksum != checksum {
            return Err(Error::Checksum { computed: computed_checksum, received: checksum });
        }

        Ok(())
    }

    fn write(&mut self, b: u8) -> Result<(), Error> {
        self.comm.write(b).map_err(|e| Error::comm(e))
    }

    fn write_response<F>(&mut self, f: F) -> Result<(), Error>
    where F: FnOnce(&mut ChecksumComm<C>) -> Result<(), C::Error> {
        self.write(b'$')?;
        let checksum = {
            let mut check = ChecksumComm::new(&mut self.comm);
            f(&mut check).map_err(Error::comm)?;
            check.into_checksum()
        };
        self.write(b'#')?;
        self.comm.write_hex(checksum).map_err(Error::comm)
    }

    fn read(&mut self) -> Result<u8, Error> {
        self.comm.read().map_err(Error::comm)
    }

    fn read_str<'b>(&mut self, buf: &'b mut [u8]) -> Result<&'b str, Error> {
        for b in buf.iter_mut() {
            *b = self.read()?;
        }

        str::from_utf8(buf).map_err(|e| {
            Error::unexpected(e.error_len().map(|_| buf[e.valid_up_to()]).unwrap_or(0), "ASCII/UTF-8 string")
        })
    }
}

/// The possible errors returned by this library.
#[derive(Debug)]
pub enum Error {
    /// Error during communication.
    CommError(Box<error::Error + Send + Sync>),

    /// An unexpected byte was received.
    Unexpected {
        byte: u8,
        expected: &'static str,
    },

    /// Received otherwise malformed data.
    Malformed,

    /// The packet checksum didn't match.
    Checksum {
        received: u8,
        computed: u8,
    },

    /// The debugger requested the retransmission of a response, which is not
    /// yet supported.
    ///
    /// Use a reliable communication channel instead.
    Nack,

    /// Target has been killed.
    ///
    /// Prior to returning this error, the library will call `StubCalls::kill`.
    ///
    /// This is not a fatal error and just indicates that the debugger closed
    /// the connection. It is not returned by `GdbStub::poll`, which instead
    /// returns `Ok(())` when the target is killed.
    Killed,
}

impl Error {
    fn comm<E>(e: E) -> Self where E: Into<Box<error::Error + Send + Sync>> {
        Error::CommError(e.into())
    }

    fn unexpected(byte: u8, expected: &'static str) -> Self {
        Error::Unexpected { byte, expected }
    }
}
