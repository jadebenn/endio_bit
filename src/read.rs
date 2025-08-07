use std::io::Read;
use std::io::Result as Res;

use crate::endian::{BE, BitEndianness, LE};

/// Reads most significant bits first.
pub type BEBitReader<R> = BitReader<BE, R>;
/// Reads least significant bits first.
pub type LEBitReader<R> = BitReader<LE, R>;

/// Adds bit-level reading support to something implementing [`std::io::Read`].
///
/// This is accomplished through an internal buffer for storing partially read bytes. Note that this buffer is for correctness, not performance - if you want to improve performance by buffering, use [`std::io::BufReader`] as the `BitReader`'s data source.
///
/// To use this reader, you'll have to choose a bit endianness to read in. The bit endianness determines the direction in which bits in a byte will be read. Note that this is distinct from byte endianness, and e.g. a format which is little endian at the byte level is not necessarily little endian at the bit level.
///
/// If you don't already know which bit endianness you need, chances are you need big endian bit numbering. In that case, just `use endio_bit::BEBitReader`. Otherwise `use endio_bit::LEBitReader`.
///
/// [`std::io::Read`]: https://doc.rust-lang.org/std/io/trait.Read.html
/// [`std::io::BufReader`]: https://doc.rust-lang.org/std/io/struct.BufReader.html
pub struct BitReader<E: BitEndianness, R: Read> {
    /// Data to read from.
    inner: R,
    /// Offset of remaining bits in a byte, 0 <= `bit_offset` < 8.
    bit_offset: u8,
    /// Storage for remaining bits after an unaligned read operation.
    bit_buffer: u8,
    phantom: std::marker::PhantomData<E>,
}

impl<E: BitEndianness, R: Read> BitReader<E, R> {
    /// Creates a new `BitReader` from something implementing [`Read`]. This will be used as the underlying object to read from.
    ///
    /// # Examples
    ///
    /// Create a `BitReader` reading from bytes in memory:
    ///
    /// ```
    /// use endio_bit::BEBitReader;
    ///
    /// let data = b"\xcf\xfe\xf3\x2c";
    /// let mut reader = BEBitReader::new(&data[..]);
    /// ```
    ///
    /// [`Read`]: https://doc.rust-lang.org/std/io/trait.Read.html
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            bit_offset: 0,
            bit_buffer: 0,
            phantom: std::marker::PhantomData,
        }
    }

    /// Returns whether the reader is aligned to the byte boundary.
    #[inline(always)]
    pub fn is_aligned(&self) -> bool {
        self.bit_offset == 0
    }

    /// Aligns to byte boundary, discarding a partial byte if the `BitReader` was not aligned.
    pub fn align(&mut self) {
        self.bit_offset = 0;
        self.bit_buffer = 0;
    }

    /// Gets a reference to the underlying reader.
    ///
    /// ```compile_fail
    /// # use endio_bit::BEBitReader;
    /// # use std::io::Read;
    /// # let mut reader = BEBitReader::new(&b"\x00"[..]);
    /// # let inner = reader.get_ref();
    /// # let mut buf = [0; 1];
    /// # inner.read(&mut buf).unwrap();
    /// ```
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Gets a mutable reference to the underlying reader.
    ///
    /// Mutable operations on the underlying reader will corrupt this `BitReader` if it is not aligned, so the reference is only returned if the `BitReader` is aligned.
    ///
    /// Panics if the `BitReader` is not aligned.
    pub fn get_mut(&mut self) -> &mut R {
        assert!(self.is_aligned(), "BitReader is not aligned");
        &mut self.inner
    }

    /// Gets a mutable reference to the underlying reader.
    ///
    /// Use with care: Any reading/seeking/etc operation on the underlying reader will corrupt this `BitReader` if it is not aligned.
    pub unsafe fn get_mut_unchecked(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Unwraps this `BitReader`, returning the underlying reader.
    ///
    /// Note that any partially read byte is lost.
    pub fn into_inner(self) -> R {
        self.inner
    }

    fn fill_buffer(&mut self) -> Res<()> {
        let mut temp = [0; 1];
        self.inner.read_exact(&mut temp)?;
        self.bit_buffer = temp[0];
        Ok(())
    }

    /// Reads a single bit, returning true for 1, false for 0.
    ///
    /// # Examples
    ///
    /// ```
    /// # use endio_bit::BEBitReader;
    /// let mut reader = BEBitReader::new(&b"\x80"[..]);
    /// let value = reader.read_bit().unwrap();
    /// assert_eq!(value, true);
    /// ```
    ///
    /// ```
    /// # use endio_bit::LEBitReader;
    /// let mut reader = LEBitReader::new(&b"\x01"[..]);
    /// let value = reader.read_bit().unwrap();
    /// assert_eq!(value, true);
    /// ```
    pub fn read_bit(&mut self) -> Res<bool> {
        if self.is_aligned() {
            self.fill_buffer()?;
        }
        let val = self.bit_buffer & (E::shift_lsb(E::shift_msb(0xff, 7), self.bit_offset)) != 0;
        self.bit_offset = (self.bit_offset + 1) % 8;
        Ok(val)
    }

    /// Reads 8 bits or less.
    ///
    /// The lowest `count` bits will be filled by this, the others will be zero.
    ///
    /// Reading more than 8 bits is intentionally not supported to keep the interface simple. Reading more can be accomplished by reading bytes and then reading any leftover bits.
    ///
    /// # Panics
    ///
    /// Panics if `count` > 8.
    ///
    /// # Examples
    ///
    /// ```
    /// # use endio_bit::BEBitReader;
    /// let mut reader = BEBitReader::new(&b"\xf8"[..]);
    /// let value = reader.read_bits(5).unwrap();
    /// assert_eq!(value, 31);
    /// ```
    ///
    /// ```
    /// # use endio_bit::LEBitReader;
    /// let mut reader = LEBitReader::new(&b"\xf8"[..]);
    /// let value = reader.read_bits(5).unwrap();
    /// assert_eq!(value, 24);
    /// ```
    pub fn read_bits(&mut self, count: u8) -> Res<u8> {
        assert!(count <= 8);
        if self.is_aligned() {
            self.fill_buffer()?;
        }
        let start = self.bit_offset;
        let end = start + count;
        let mut res = E::shift_msb(self.bit_buffer, start);
        if end > 8 {
            self.fill_buffer()?;
            res |= E::shift_lsb(self.bit_buffer, 8 - start);
        }
        res = E::shift_lsb(res, 8 - count);
        res = E::align_right(res, count);
        self.bit_offset = end % 8;
        Ok(res)
    }
}

/// Read bytes from a `BitReader` just like from [`Read`], but with bit shifting support for unaligned reads.
///
/// Directly maps to [`Read`] for aligned reads.
///
/// [`Read`]: https://doc.rust-lang.org/std/io/trait.Read.html
impl<E: BitEndianness, R: Read> Read for BitReader<E, R> {
    fn read(&mut self, buf: &mut [u8]) -> Res<usize> {
        let count_read = self.inner.read(buf)?;
        if self.is_aligned() {
            return Ok(count_read);
        }
        let mut last_byte = self.bit_buffer;
        for b in buf.iter_mut() {
            let current_byte = *b;
            *b = E::shift_msb(last_byte, self.bit_offset)
                | E::shift_lsb(current_byte, 8 - self.bit_offset);
            last_byte = current_byte;
        }
        self.bit_buffer = last_byte;
        Ok(count_read)
    }
}

#[cfg(test)]
mod tests_common {
    use crate::BEBitReader;
    use std::io::Read;

    #[test]
    fn get_ref() {
        let reader = BEBitReader::new(&b"\xf8"[..]);
        let inner = reader.get_ref();
        assert_eq!(inner[0], 0xf8);
    }

    #[test]
    fn get_mut_aligned() {
        let mut reader = BEBitReader::new(&b"\xf8"[..]);
        let inner = reader.get_mut();
        let mut buf = [0; 1];
        inner.read(&mut buf).unwrap();
        assert_eq!(buf[0], 0xf8);
    }

    #[test]
    #[should_panic]
    fn get_mut_unaligned() {
        let data = &b"\xff"[..];
        let mut reader = BEBitReader::new(data);
        reader.read_bits(4).unwrap();
        reader.get_mut();
    }

    #[test]
    fn get_mut_unchecked() {
        let mut reader = BEBitReader::new(&b"\x00\xff"[..]);
        reader.read_bits(4).unwrap();
        let inner = unsafe { reader.get_mut_unchecked() };
        let mut buf = [0; 1];
        inner.read(&mut buf).unwrap();
        assert_eq!(buf[0], 0xff);
    }

    #[test]
    fn into_inner() {
        let reader = BEBitReader::new(std::io::empty());
        let inner = reader.into_inner();
        inner.bytes();
    }

    #[test]
    fn align() {
        let mut reader = BEBitReader::new(&b"\xf8\x80"[..]);
        let bits = reader.read_bits(5).unwrap();
        assert!(!reader.is_aligned());
        reader.align();
        assert!(reader.is_aligned());
        let bit = reader.read_bit().unwrap();
        assert_eq!(bits, 31);
        assert!(bit);
    }
}

#[cfg(test)]
mod tests_be {
    use crate::BEBitReader;
    use std::io::Read;

    #[test]
    fn read_aligned() {
        let mut reader = BEBitReader::new(&b"Test"[..]);
        let mut buf = [0; 4];
        assert_eq!(reader.read(&mut buf).unwrap(), 4);
        assert_eq!(&buf, b"Test");
    }

    #[test]
    fn read_shifted() {
        let mut reader = BEBitReader::new(&b"\xaa\x8c\xae\x6e\x80"[..]);
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        let mut buf = [0; 0];
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
        assert_eq!(&buf, b"");
        let mut buf = [0; 1];
        assert_eq!(reader.read(&mut buf).unwrap(), 1);
        assert_eq!(&buf, b"T");
        let mut buf = [0; 7];
        assert_eq!(reader.read(&mut buf).unwrap(), 3);
        assert_eq!(&buf, b"est\0\0\0\0");
    }

    #[test]
    fn read_bit() {
        let mut reader = BEBitReader::new(&b"\x2a"[..]);
        assert!(!reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
    }

    #[test]
    fn read_bits() {
        let mut reader = BEBitReader::new(&b"\xab\xcd"[..]);
        assert_eq!(reader.read_bits(4).unwrap(), 0x0a);
        assert_eq!(reader.read_bits(8).unwrap(), 0xbc);
    }

    #[test]
    #[should_panic]
    fn read_too_many_bits() {
        let mut reader = BEBitReader::new(&b""[..]);
        let _ = reader.read_bits(9);
    }
}

#[cfg(test)]
mod tests_le {
    use crate::LEBitReader;
    use std::io::Read;

    #[test]
    fn read_aligned() {
        let mut reader = LEBitReader::new(&b"Test"[..]);
        let mut buf = [0; 4];
        assert_eq!(reader.read(&mut buf).unwrap(), 4);
        assert_eq!(&buf, b"Test");
    }

    #[test]
    fn read_shifted() {
        let mut reader = LEBitReader::new(&b"\xaa\x8c\xae\x6e\x80"[..]);
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        let mut buf = [0; 0];
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
        assert_eq!(&buf, b"");
        let mut buf = [0; 1];
        assert_eq!(reader.read(&mut buf).unwrap(), 1);
        assert_eq!(&buf, b"\x95");
        let mut buf = [0; 7];
        assert_eq!(reader.read(&mut buf).unwrap(), 3);
        assert_eq!(&buf, b"\xd1\xd5\x0d\x10\0\0\0");
    }

    #[test]
    fn read_bit() {
        let mut reader = LEBitReader::new(&b"\x2a"[..]);
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
    }

    #[test]
    fn read_bits() {
        let mut reader = LEBitReader::new(&b"\xab\xcd"[..]);
        assert_eq!(reader.read_bits(4).unwrap(), 0x0b);
        assert_eq!(reader.read_bits(8).unwrap(), 0xda);
    }

    #[test]
    #[should_panic]
    fn read_too_many_bits() {
        let mut reader = LEBitReader::new(&b""[..]);
        let _ = reader.read_bits(9);
    }
}
