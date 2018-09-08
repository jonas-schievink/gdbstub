use std::str::{self, Utf8Error};
use std::num::ParseIntError;

pub fn hex_decode_in_place(bytes: &mut [u8]) -> Result<&[u8], HexDecodeError> {
    for i in 0..bytes.len()/2 {
        bytes[i] = u8::from_str_radix(str::from_utf8(&bytes[i*2..i*2+2])?, 16)?;
    }
    Ok(&bytes[..bytes.len()/2])
}

pub enum HexDecodeError {
    Utf8Error(Utf8Error),
    ParseIntError(ParseIntError),
}

impl From<Utf8Error> for HexDecodeError {
    fn from(e: Utf8Error) -> Self {
        HexDecodeError::Utf8Error(e)
    }
}

impl From<ParseIntError> for HexDecodeError {
    fn from(e: ParseIntError) -> Self {
        HexDecodeError::ParseIntError(e)
    }
}
