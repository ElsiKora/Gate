//! The process protocol: frames of a little-endian `u32` length, one kind byte and the payload.
//! Kind 0 is a JSON message, kind 1 raw bytes, kind 2 "none".

use std::io::{self, Read, Write};

pub const JSON: u8 = 0;
pub const BYTES: u8 = 1;
pub const NONE: u8 = 2;

/// Writes one frame and flushes it.
///
/// # Errors
/// Fails when the pipe is closed.
pub fn write(out: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    let length = u32::try_from(payload.len() + 1)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a frame over 4 GiB"))?;
    out.write_all(&length.to_le_bytes())?;
    out.write_all(&[kind])?;
    out.write_all(payload)?;
    out.flush()
}

/// Reads one frame: its kind and payload; `None` at the end of the stream.
///
/// # Errors
/// Fails on a truncated frame.
pub fn read(input: &mut impl Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut length = [0; 4];
    match input.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_le_bytes(length) as usize;
    let mut frame = vec![0; length];
    input.read_exact(&mut frame)?;
    let kind = frame.first().copied().unwrap_or(NONE);
    frame.remove(0);
    Ok(Some((kind, frame)))
}
