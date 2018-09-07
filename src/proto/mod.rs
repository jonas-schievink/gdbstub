use std::{str, u64};

use std::str::Utf8Error;
use std::num::{ParseIntError, NonZeroU32};

#[derive(Debug)]
pub enum ThreadAction {
    /// Continue / Step.
    ContStep,
    Other,
}

/// A command received from a connected GDB.
#[derive(Debug)]
pub enum Command<'a> {
    /// `?`
    GetHaltReason,
    /// `g` - Read general processor registers.
    ReadRegisters,
    /// `k` - Kill target program or system and disconnect.
    Kill,
    /// `m` - Read data from memory.
    ReadMem {
        // FIXME: Replace `u64` with something... better... dunno.
        start: u64,
        len: u64,
    },
    WriteMem {
        start: u64,
        bytes: &'a [u8],
    },
    SetThread {
        action: ThreadAction,
        thread: ThreadId,
    },
}

impl<'a> Command<'a> {
    pub fn parse(buf: &'a mut [u8]) -> Result<Self, ParseError> {
        // FIXME: This can panic if the packet isn't as long as we expect.

        if buf.is_empty() {
            return Err(ParseError::Malformed);
        }

        match buf[0] {
            b'v' => {
                let name = buf[1..].splitn(2, |b| *b == b';').next().ok_or(ParseError::Malformed)?;
                let name = str::from_utf8(name)?;
                trace!("v{}", name);
                match name {
                    _ => {
                        debug!("unsupported v-command 'v{}'", name);
                        Err(ParseError::Unsupported)
                    }
                }
            }
            m @ b'm' | m @ b'M' => {
                let mut parts = buf[1..].splitn_mut(3, |b| *b == b',');
                let start = u64::from_str_radix(str::from_utf8(parts.next().unwrap())?, 16)?;
                let len = u64::from_str_radix(str::from_utf8(parts.next().ok_or(ParseError::Malformed)?)?, 16)?;

                if m == b'm' {
                    Ok(Command::ReadMem { start, len })
                } else {
                    // hex-decode the bytes to be written
                    let mut bytes = parts.next().ok_or(ParseError::Malformed)?;
                    // do a little trick to reuse the buffer we were passed
                    // store the decoded bytes in the first part of `bytes`
                    // while decoding 2 bytes (hex digits) at a time
                    for i in 0..bytes.len()/2 {
                        bytes[i] = u8::from_str_radix(str::from_utf8(&bytes[i*2..i*2+2])?, 16)?;
                    }
                    let bytes = &bytes[..bytes.len()/2];

                    if bytes.len() != len as usize {
                        error!("M command len={}, number of bytes={}", len, bytes.len());
                        return Err(ParseError::Malformed);
                    }

                    Ok(Command::WriteMem { start, bytes })
                }
            }
            b'H' => {
                let action = match buf[1] as char {
                    'c' => ThreadAction::ContStep,
                    'g' => ThreadAction::Other,
                    invalid => {
                        error!("invalid action for H command: {}", invalid);
                        return Err(ParseError::Malformed)
                    },
                };
                let thread = ThreadId::parse(&buf[2..])?;

                Ok(Command::SetThread { action, thread })
            }
            b'?' => Ok(Command::GetHaltReason),
            b'g' => Ok(Command::ReadRegisters),
            b'k' => Ok(Command::Kill),
            unknown => {
                debug!("unsupported command '{}'", unknown as char);
                Err(ParseError::Unsupported)
            },
        }
    }
}

#[derive(Debug)]
pub enum ThreadId {
    All,
    Any,
    Thread(NonZeroU32),
}

impl ThreadId {
    fn parse(buf: &[u8]) -> Result<Self, ParseError> {
        match buf {
            b"-1" => Ok(ThreadId::All),
            b"0" => Ok(ThreadId::Any),
            _ => {
                // big-endian hex string indicating the thread ID
                let id = u32::from_be(u32::from_str_radix(str::from_utf8(buf)?, 16)?);
                Ok(ThreadId::Thread(NonZeroU32::new(id).ok_or(ParseError::Malformed)?))
            }
        }
    }
}

pub enum ParseError {
    /// The data is malformed, indicating a problem with communication or the
    /// connected debugger.
    Malformed,

    /// An unknown packet/command was encountered.
    ///
    /// The gdbstub implementor should reply with an empty message to indicate
    /// that the operation is not supported.
    Unsupported,
}

impl From<ParseIntError> for ParseError {
    fn from(_: ParseIntError) -> Self {
        ParseError::Malformed
    }
}

impl From<Utf8Error> for ParseError {
    fn from(_: Utf8Error) -> Self {
        ParseError::Malformed
    }
}
