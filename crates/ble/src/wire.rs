// SPDX-License-Identifier: Apache-2.0

//! Little-endian byte helpers shared by the protocol codec.
//!
//! The SNF telemetry protocol is defined as an explicit little-endian byte
//! layout (see `PROTOCOL.md` §4), never as serialized Rust structs. These
//! helpers keep the payload encoders in [`crate::protocol`] terse while making
//! the wire order the single source of truth: encoders push fields in offset
//! order, and the [`Reader`] used to parse client-written Stream Control frames
//! reads them back the same way with explicit bounds checks.

/// Append-only writer that lays fields out in little-endian, offset order.
///
/// There is deliberately no seeking: a payload is written front to back, so the
/// resulting buffer matches the offset tables in `PROTOCOL.md` by construction.
#[derive(Debug, Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    /// A writer that will hold exactly `capacity` bytes (the fixed size of most
    /// payloads), avoiding reallocation as fields are pushed.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
        }
    }

    pub fn u8(&mut self, value: u8) -> &mut Self {
        self.buf.push(value);
        self
    }

    pub fn i8(&mut self, value: i8) -> &mut Self {
        self.buf.push(value as u8);
        self
    }

    pub fn u16(&mut self, value: u16) -> &mut Self {
        self.buf.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn i16(&mut self, value: i16) -> &mut Self {
        self.buf.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn u32(&mut self, value: u32) -> &mut Self {
        self.buf.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Write `count` zero bytes. Reserved fields are always transmitted as zero
    /// (`PROTOCOL.md` §4); receivers ignore them.
    pub fn zeros(&mut self, count: usize) -> &mut Self {
        self.buf.resize(self.buf.len() + count, 0);
        self
    }

    /// Copy a byte slice verbatim (magic strings, echo payloads).
    pub fn bytes(&mut self, value: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(value);
        self
    }

    /// Number of bytes written so far.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Consume the writer, yielding the finished buffer.
    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }
}

/// Cursor over an untrusted input buffer (a client Stream Control write).
///
/// Every accessor is bounds-checked and returns `None` past the end, so a short
/// or malformed request is rejected rather than panicking.
#[derive(Debug)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Bytes not yet consumed.
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    pub fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|s| s[0])
    }

    pub fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|s| u16::from_le_bytes([s[0], s[1]]))
    }
}
