use std::error;
use std::fmt;

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
    pub(crate) fn comm<E>(e: E) -> Self where E: Into<Box<error::Error + Send + Sync>> {
        Error::CommError(e.into())
    }

    pub(crate) fn unexpected(byte: u8, expected: &'static str) -> Self {
        Error::Unexpected { byte, expected }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::CommError(e) => write!(f, "communication error: {}", e),
            Error::Unexpected { byte, expected } => write!(f, "unexpected byte {} ({:02X}/{}), expected {}", byte, byte, *byte as char, expected),
            Error::Malformed => write!(f, "malformed packet"),
            Error::Checksum { received, computed } => write!(f, "incorrect checksum, got {:02X}, expected {:02X}", received, computed),
            Error::Nack => write!(f, "debugger did not acknowledge answer"),
            Error::Killed => write!(f, "the target process has been killed"),
        }
    }
}

impl error::Error for Error {}
