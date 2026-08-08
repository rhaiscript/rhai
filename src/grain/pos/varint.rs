//! LEB128, the encoding everything in a rhaigrain artifact is counted in.
//!
//! Small numbers dominate a chunk — pool indices, jump targets, line numbers —
//! and almost all of them fit in one byte. Signed values are zigzagged first,
//! so a small negative costs no more than a small positive.
//!
//! Decoding takes the cursor by reference rather than owning one, so the same
//! functions serve a reader that tracks its own errors and a resolver that has
//! nothing but a byte slice.

/// Why a varint could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The bytes ran out mid-value.
    Truncated,
    /// A value wider than 64 bits, or a run of continuation bytes that never
    /// ends. Refusing both is what stops a corrupt table from spinning.
    Overflow,
}

/// Read an unsigned varint, advancing `at`.
///
/// # Errors
///
/// [`Error::Truncated`] if the slice ends first, [`Error::Overflow`] if the
/// value does not fit in 64 bits.
pub fn u64(bytes: &[u8], at: &mut usize) -> Result<u64, Error> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at).ok_or(Error::Truncated)?;
        *at += 1;
        let payload = u64::from(byte & 0x7f);
        // The tenth group has a single bit left to land in. Shifting would drop
        // the other six rather than refuse them, so a value too wide for 64 bits
        // would decode as a smaller one.
        if shift == 63 && payload > 1 {
            return Err(Error::Overflow);
        }
        value |= payload << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(Error::Overflow)
}

/// Read a zigzagged signed varint, advancing `at`.
///
/// # Errors
///
/// As [`u64`](fn@u64).
pub fn i64(bytes: &[u8], at: &mut usize) -> Result<i64, Error> {
    let raw = u64(bytes, at)?;
    Ok(((raw >> 1) as i64) ^ -((raw & 1) as i64))
}

/// Read an unsigned varint that must fit a `u32`, advancing `at`.
///
/// # Errors
///
/// As [`u64`](fn@u64), plus [`Error::Overflow`] if the value is too wide for
/// the field.
pub fn u32(bytes: &[u8], at: &mut usize) -> Result<u32, Error> {
    core::convert::TryFrom::try_from(u64(bytes, at)?).map_err(|_| Error::Overflow)
}

/// Append an unsigned varint.
pub fn put_u64(out: &mut alloc::vec::Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Append a zigzagged signed varint.
pub fn put_i64(out: &mut alloc::vec::Vec<u8>, value: i64) {
    put_u64(out, ((value << 1) ^ (value >> 63)) as u64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn unsigned_values_round_trip_at_the_edges() {
        for value in [0u64, 1, 127, 128, 16383, 16384, u32::MAX as u64, u64::MAX] {
            let mut buf = Vec::new();
            put_u64(&mut buf, value);
            let mut at = 0;
            assert_eq!(u64(&buf, &mut at), Ok(value), "at {value}");
            assert_eq!(
                at,
                buf.len(),
                "the reader must consume exactly what was written"
            );
        }
    }

    #[test]
    fn signed_values_round_trip_across_zero() {
        for value in [0i64, -1, 1, -64, 63, i32::MIN as i64, i64::MIN, i64::MAX] {
            let mut buf = Vec::new();
            put_i64(&mut buf, value);
            let mut at = 0;
            assert_eq!(i64(&buf, &mut at), Ok(value), "at {value}");
        }
    }

    /// The encoding only pays for itself if the common case is one byte.
    #[test]
    fn values_under_128_cost_one_byte() {
        let mut buf = Vec::new();
        put_u64(&mut buf, 127);
        assert_eq!(buf.len(), 1);
        put_i64(&mut buf, -64);
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn a_run_of_continuation_bytes_terminates() {
        let mut at = 0;
        assert_eq!(u64(&[0xff; 64], &mut at), Err(Error::Overflow));
    }

    #[test]
    fn running_out_of_bytes_is_an_error_not_a_panic() {
        let mut at = 0;
        assert_eq!(u64(&[], &mut at), Err(Error::Truncated));
        let mut at = 0;
        assert_eq!(u64(&[0x80], &mut at), Err(Error::Truncated));
    }

    /// The tenth group is the one place a shift could silently lose bits, so a
    /// wide value there must be refused rather than truncated into a small one.
    #[test]
    fn a_tenth_group_wider_than_one_bit_is_refused() {
        let mut ten = [0x80u8; 10];

        // The largest value there is: nine full groups and a final bit.
        ten[9] = 0x01;
        let mut at = 0;
        assert_eq!(
            u64(&[[0xffu8; 9].as_slice(), &[0x01]].concat(), &mut at),
            Ok(u64::MAX)
        );

        // One past it. Shifting would drop the payload and read this as zero.
        ten[9] = 0x02;
        let mut at = 0;
        assert_eq!(u64(&ten, &mut at), Err(Error::Overflow));

        ten[9] = 0x7f;
        let mut at = 0;
        assert_eq!(u64(&ten, &mut at), Err(Error::Overflow));
    }

    #[test]
    fn a_value_too_wide_for_its_field_is_refused() {
        let mut buf = Vec::new();
        put_u64(&mut buf, u64::from(u32::MAX) + 1);
        let mut at = 0;
        assert_eq!(u32(&buf, &mut at), Err(Error::Overflow));
    }
}
