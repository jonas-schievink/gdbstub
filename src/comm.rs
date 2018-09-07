use std::{error, io};
use std::io::prelude::*;

/// A communication channel between the stub and a connecting GDB instance.
///
/// This is a bytewise bidirectional transport comparable to `Read + Write`. It
/// is hence implemented automatically for anything that implements both `Read`
/// and `Write` (eg. `TcpStream`).
pub trait Comm {
    /// Error type returned when reading or writing fails.
    type Error: Into<Box<error::Error + Send + Sync>>;

    /// Read a byte from the connected debugger.
    fn read(&mut self) -> Result<u8, Self::Error>;

    /// Send a byte to the connected debugger.
    fn write(&mut self, byte: u8) -> Result<(), Self::Error>;

    /// Writes all bytes from a slice to the stream.
    fn write_all(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        for b in data {
            self.write(*b)?;
        }

        Ok(())
    }

    /// Writes a byte as a hex string.
    fn write_hex(&mut self, byte: u8) -> Result<(), Self::Error> {
        let mut hex_str = [0u8, 0];
        write!(&mut hex_str[..], "{:02x}", byte).unwrap();
        self.write(hex_str[0])?;
        self.write(hex_str[1])?;
        Ok(())
    }

    /// Writes all bytes in `data` as hexadecimal-encoded strings.
    fn write_all_hex(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        for b in data {
            self.write_hex(*b)?;
        }

        Ok(())
    }
}

impl<T> Comm for T
    where T: Read + Write {
    type Error = io::Error;

    fn read(&mut self) -> io::Result<u8> {
        let mut buf = [0u8];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    fn write(&mut self, byte: u8) -> io::Result<()> {
        self.write_all(&[byte])
    }
}

/// A `Comm` decorator that computes the checksum of all passing data.
pub struct ChecksumComm<'a, C: 'a> {
    inner: &'a mut C,
    checksum: u8,
}

impl<'a, C: Comm + 'a> ChecksumComm<'a, C> {
    pub fn new(inner: &'a mut C) -> Self {
        Self {
            inner,
            checksum: 0,
        }
    }

    pub fn into_checksum(self) -> u8 {
        self.checksum
    }
}

impl<'a, C: Comm + 'a> Comm for ChecksumComm<'a, C> {
    type Error = C::Error;

    fn read(&mut self) -> Result<u8, C::Error> {
        self.inner.read()
    }

    fn write(&mut self, byte: u8) -> Result<(), C::Error> {
        self.checksum = self.checksum.wrapping_add(byte);
        self.inner.write(byte)
    }
}
