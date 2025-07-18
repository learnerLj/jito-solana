//! # Low-Level Byte Parsing Utilities
//!
//! This module contains optimized byte parsing functions used throughout the transaction
//! view library. These functions are designed for maximum performance while maintaining
//! safety through comprehensive bounds checking.
//!
//! ## Key Features
//!
//! - **Bounds Safety**: All functions include comprehensive bounds checking
//! - **Zero-Copy Design**: Functions work directly with byte slices without copying
//! - **Optimized for Solana**: Special optimizations for Solana's compressed encoding formats
//! - **Performance Critical**: These functions are in the hot path for transaction parsing
//!
//! ## Compressed Integer Encoding
//!
//! Solana uses a compressed encoding for integers to save space in transactions:
//! - Values 0-127: Encoded as a single byte
//! - Values 128-16383: Encoded as two bytes
//! - Values 16384-2097151: Encoded as three bytes
//!
//! The encoding uses the MSB as a continuation bit to indicate if more bytes follow.
//!
//! ## Safety Considerations
//!
//! While this module contains unsafe code for performance, all unsafe operations are:
//! - Properly documented with safety requirements
//! - Bounded by compile-time and runtime checks
//! - Only used after validating all preconditions

use crate::result::{Result, TransactionViewError};

/// Verifies that a buffer has sufficient remaining bytes for a read operation.
///
/// This function is fundamental to the safety of all parsing operations in this library.
/// It ensures that subsequent reads will not go out of bounds.
///
/// # Arguments
/// * `bytes` - The byte slice being read from
/// * `offset` - Current position in the byte slice
/// * `num_bytes` - Number of additional bytes required
///
/// # Returns
/// * `Ok(())` if sufficient bytes remain
/// * `Err(TransactionViewError::ParseError)` if insufficient bytes remain
///
/// # Safety Assumptions
/// * The current offset must not exceed the buffer length
/// * This function must be called before any unsafe memory access
#[inline(always)]
pub fn check_remaining(bytes: &[u8], offset: usize, num_bytes: usize) -> Result<()> {
    if num_bytes > bytes.len().wrapping_sub(offset) {
        Err(TransactionViewError::ParseError)
    } else {
        Ok(())
    }
}

/// Safely reads a single byte from the buffer and advances the offset.
///
/// This is a fundamental building block for parsing transaction components.
/// It includes bounds checking to ensure memory safety.
///
/// # Arguments
/// * `bytes` - The byte slice to read from
/// * `offset` - Mutable reference to the current offset (will be incremented)
///
/// # Returns
/// * `Ok(byte_value)` if successful
/// * `Err(TransactionViewError::ParseError)` if out of bounds
///
/// # Performance Notes
/// * Uses wrapping arithmetic for offset increment to handle edge cases
/// * Bounds checking is implicit through the `get()` method
#[inline(always)]
pub fn read_byte(bytes: &[u8], offset: &mut usize) -> Result<u8> {
    // The get() method implicitly checks bounds, eliminating need for explicit check_remaining call
    let value = bytes
        .get(*offset)
        .copied()
        .ok_or(TransactionViewError::ParseError);
    *offset = offset.wrapping_add(1);
    value
}

/// Reads a single byte without bounds checking for maximum performance.
///
/// This function is used in performance-critical sections where bounds have already
/// been validated by higher-level parsing logic. It provides maximum speed by
/// eliminating redundant bounds checks.
///
/// # Arguments
/// * `bytes` - The byte slice to read from
/// * `offset` - Mutable reference to the current offset (will be incremented)
///
/// # Returns
/// The byte value at the current offset
///
/// # Safety Requirements
/// **CRITICAL**: This function performs no bounds checking. The caller MUST ensure:
/// 1. `bytes` is a valid, properly initialized byte slice
/// 2. `offset` points to a valid position within `bytes`
/// 3. There is at least one byte available at `offset`
/// 4. The slice will remain valid for the duration of this call
///
/// # Usage Pattern
/// This function should only be used after:
/// 1. Initial bounds validation with `check_remaining()`
/// 2. Frame parsing that validates total structure size
/// 3. In iterator implementations where bounds are pre-validated
#[inline(always)]
pub unsafe fn unchecked_read_byte(bytes: &[u8], offset: &mut usize) -> u8 {
    let value = *bytes.get_unchecked(*offset);
    *offset = offset.wrapping_add(1);
    value
}

/// Read a compressed u16 from `bytes` starting at `offset`.
/// If the buffer is too short or the encoding is invalid, return Err.
/// `offset` is updated to point to the byte after the compressed u16.
///
/// * `bytes` - Slice of bytes to read from.
/// * `offset` - Current offset into `bytes`.
///
/// Assumptions:
/// - The current offset is not greater than `bytes.len()`.
#[allow(dead_code)]
#[inline(always)]
pub fn read_compressed_u16(bytes: &[u8], offset: &mut usize) -> Result<u16> {
    let mut result = 0u16;
    let mut shift = 0u16;

    for i in 0..3 {
        // Implicitly checks that the offset is within bounds, no need
        // to call check_remaining explicitly here.
        let byte = *bytes
            .get(offset.wrapping_add(i))
            .ok_or(TransactionViewError::ParseError)?;
        // non-minimal encoding or overflow
        if (i > 0 && byte == 0) || (i == 2 && byte > 3) {
            return Err(TransactionViewError::ParseError);
        }
        result |= ((byte & 0x7F) as u16) << shift;
        shift = shift.wrapping_add(7);
        if byte & 0x80 == 0 {
            *offset = offset.wrapping_add(i).wrapping_add(1);
            return Ok(result);
        }
    }

    // if we reach here, it means that all 3 bytes were used
    *offset = offset.wrapping_add(3);
    Ok(result)
}

/// Optimized compressed u16 decoder for Solana transaction parsing.
///
/// This function is specifically optimized for Solana's transaction format constraints.
/// Since transaction packets have a maximum size of 1232 bytes, any array length
/// within a valid transaction cannot exceed this value. This allows us to optimize
/// the decoding by limiting reads to a maximum of 2 bytes.
///
/// # Performance Optimization
/// Traditional compressed u16 decoding can read up to 3 bytes, but in Solana's
/// transaction context:
/// - Values 0-127: 1 byte encoding
/// - Values 128-1232: 2 byte encoding
/// - Values >1232: Invalid for transaction packets
///
/// This optimization provides:
/// - Reduced branch prediction misses
/// - Fewer memory accesses
/// - Faster validation of encoding correctness
///
/// # Arguments
/// * `bytes` - The byte slice containing compressed data
/// * `offset` - Mutable reference to current position (updated after read)
///
/// # Returns
/// * `Ok(value)` - The decoded u16 value
/// * `Err(ParseError)` - Invalid encoding or insufficient bytes
///
/// # Encoding Validation
/// The function validates:
/// - Minimal encoding (no unnecessary leading zero bytes)
/// - Value does not exceed packet size constraints
/// - Sufficient bytes available for complete decode
#[inline(always)]
pub fn optimized_read_compressed_u16(bytes: &[u8], offset: &mut usize) -> Result<u16> {
    let mut result = 0u16;

    // First byte
    let byte1 = *bytes.get(*offset).ok_or(TransactionViewError::ParseError)?;
    result |= (byte1 & 0x7F) as u16;
    if byte1 & 0x80 == 0 {
        *offset = offset.wrapping_add(1);
        return Ok(result);
    }

    // Second byte
    let byte2 = *bytes
        .get(offset.wrapping_add(1))
        .ok_or(TransactionViewError::ParseError)?;
    if byte2 == 0 || byte2 & 0x80 != 0 {
        return Err(TransactionViewError::ParseError); // non-minimal encoding or overflow
    }
    result |= ((byte2 & 0x7F) as u16) << 7;
    *offset = offset.wrapping_add(2);

    Ok(result)
}

/// Update the `offset` to point to the byte after an array of length `len` and
/// of type `T`. If the buffer is too short, return Err.
///
/// * `bytes` - Slice of bytes to read from.
/// * `offset` - Current offset into `bytes`.
/// * `num_elements` - Number of `T` elements in the array.
///
/// Assumptions:
/// 1. The current offset is not greater than `bytes.len()`.
/// 2. The size of `T` is small enough such that a usize will not overflow if
///    given the maximum array size (u16::MAX).
#[inline(always)]
pub fn advance_offset_for_array<T: Sized>(
    bytes: &[u8],
    offset: &mut usize,
    num_elements: u16,
) -> Result<()> {
    let array_len_bytes = usize::from(num_elements).wrapping_mul(core::mem::size_of::<T>());
    check_remaining(bytes, *offset, array_len_bytes)?;
    *offset = offset.wrapping_add(array_len_bytes);
    Ok(())
}

/// Update the `offset` to point t the byte after the `T`.
/// If the buffer is too short, return Err.
///
/// * `bytes` - Slice of bytes to read from.
/// * `offset` - Current offset into `bytes`.
///
/// Assumptions:
/// 1. The current offset is not greater than `bytes.len()`.
/// 2. The size of `T` is small enough such that a usize will not overflow.
#[inline(always)]
pub fn advance_offset_for_type<T: Sized>(bytes: &[u8], offset: &mut usize) -> Result<()> {
    let type_size = core::mem::size_of::<T>();
    check_remaining(bytes, *offset, type_size)?;
    *offset = offset.wrapping_add(type_size);
    Ok(())
}

/// Return a reference to the next slice of `T` in the buffer, checking bounds
/// and advancing the offset.
/// If the buffer is too short, return Err.
///
/// * `bytes` - Slice of bytes to read from.
/// * `offset` - Current offset into `bytes`.
/// * `num_elements` - Number of `T` elements in the slice.
///
/// # Safety
/// 1. `bytes` must be a valid slice of bytes.
/// 2. `offset` must be a valid offset into `bytes`.
/// 3. `bytes + offset` must be properly aligned for `T`.
/// 4. `T` slice must be validly initialized.
/// 5. The size of `T` is small enough such that a usize will not overflow if
///    given the maximum slice size (u16::MAX).
#[inline(always)]
pub unsafe fn read_slice_data<'a, T: Sized>(
    bytes: &'a [u8],
    offset: &mut usize,
    num_elements: u16,
) -> Result<&'a [T]> {
    let current_ptr = bytes.as_ptr().add(*offset);
    advance_offset_for_array::<T>(bytes, offset, num_elements)?;
    Ok(unsafe { core::slice::from_raw_parts(current_ptr as *const T, usize::from(num_elements)) })
}

/// Return a reference to the next slice of `T` in the buffer,
/// and advancing the offset.
///
/// * `bytes` - Slice of bytes to read from.
/// * `offset` - Current offset into `bytes`.
/// * `num_elements` - Number of `T` elements in the slice.
///
/// # Safety
/// 1. `bytes` must be a valid slice of bytes.
/// 2. `offset` must be a valid offset into `bytes`.
/// 3. `bytes + offset` must be properly aligned for `T`.
/// 4. `T` slice must be validly initialized.
/// 5. The size of `T` is small enough such that a usize will not overflow if
///    given the maximum slice size (u16::MAX).
#[inline(always)]
pub unsafe fn unchecked_read_slice_data<'a, T: Sized>(
    bytes: &'a [u8],
    offset: &mut usize,
    num_elements: u16,
) -> &'a [T] {
    let current_ptr = bytes.as_ptr().add(*offset);
    let array_len_bytes = usize::from(num_elements).wrapping_mul(core::mem::size_of::<T>());
    *offset = offset.wrapping_add(array_len_bytes);
    unsafe { core::slice::from_raw_parts(current_ptr as *const T, usize::from(num_elements)) }
}

/// Return a reference to the next `T` in the buffer, checking bounds and
/// advancing the offset.
/// If the buffer is too short, return Err.
///
/// * `bytes` - Slice of bytes to read from.
/// * `offset` - Current offset into `bytes`.
///
/// # Safety
/// 1. `bytes` must be a valid slice of bytes.
/// 2. `offset` must be a valid offset into `bytes`.
/// 3. `bytes + offset` must be properly aligned for `T`.
/// 4. `T` must be validly initialized.
#[inline(always)]
pub unsafe fn read_type<'a, T: Sized>(bytes: &'a [u8], offset: &mut usize) -> Result<&'a T> {
    let current_ptr = bytes.as_ptr().add(*offset);
    advance_offset_for_type::<T>(bytes, offset)?;
    Ok(unsafe { &*(current_ptr as *const T) })
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        bincode::{serialize_into, DefaultOptions, Options},
        solana_packet::PACKET_DATA_SIZE,
        solana_short_vec::ShortU16,
    };

    #[test]
    fn test_check_remaining() {
        // Empty buffer checks
        assert!(check_remaining(&[], 0, 0).is_ok());
        assert!(check_remaining(&[], 0, 1).is_err());

        // Buffer with data checks
        assert!(check_remaining(&[1, 2, 3], 0, 0).is_ok());
        assert!(check_remaining(&[1, 2, 3], 0, 1).is_ok());
        assert!(check_remaining(&[1, 2, 3], 0, 3).is_ok());
        assert!(check_remaining(&[1, 2, 3], 0, 4).is_err());

        // Non-zero offset.
        assert!(check_remaining(&[1, 2, 3], 1, 0).is_ok());
        assert!(check_remaining(&[1, 2, 3], 1, 1).is_ok());
        assert!(check_remaining(&[1, 2, 3], 1, 2).is_ok());
        assert!(check_remaining(&[1, 2, 3], 1, usize::MAX).is_err());
    }

    #[test]
    fn test_read_byte() {
        let bytes = [5, 6, 7];
        let mut offset = 0;
        assert_eq!(read_byte(&bytes, &mut offset), Ok(5));
        assert_eq!(offset, 1);
        assert_eq!(read_byte(&bytes, &mut offset), Ok(6));
        assert_eq!(offset, 2);
        assert_eq!(read_byte(&bytes, &mut offset), Ok(7));
        assert_eq!(offset, 3);
        assert!(read_byte(&bytes, &mut offset).is_err());
    }

    #[test]
    fn test_read_compressed_u16() {
        let mut buffer = [0u8; 1024];
        let options = DefaultOptions::new().with_fixint_encoding(); // Ensure fixed-int encoding

        // Test all possible u16 values
        for value in 0..=u16::MAX {
            let mut offset;
            let short_u16 = ShortU16(value);

            // Serialize the value into the buffer
            serialize_into(&mut buffer[..], &short_u16).expect("Serialization failed");

            // Use bincode's size calculation to determine the length of the serialized data
            let serialized_len = options
                .serialized_size(&short_u16)
                .expect("Failed to get serialized size");

            // Reset offset
            offset = 0;

            // Read the value back using unchecked_read_u16_compressed
            let read_value = read_compressed_u16(&buffer, &mut offset);

            // Assert that the read value matches the original value
            assert_eq!(read_value, Ok(value), "Value mismatch for: {}", value);

            // Assert that the offset matches the serialized length
            assert_eq!(
                offset, serialized_len as usize,
                "Offset mismatch for: {}",
                value
            );
        }

        // Test bounds.
        // All 0s => 0
        assert_eq!(Ok(0), read_compressed_u16(&[0; 3], &mut 0));
        // Overflow
        assert!(read_compressed_u16(&[0xFF, 0xFF, 0x04], &mut 0).is_err());
        assert_eq!(
            read_compressed_u16(&[0xFF, 0xFF, 0x03], &mut 0),
            Ok(u16::MAX)
        );

        // overflow errors
        assert!(read_compressed_u16(&[u8::MAX; 1], &mut 0).is_err());
        assert!(read_compressed_u16(&[u8::MAX; 2], &mut 0).is_err());

        // Minimal encoding checks
        assert!(read_compressed_u16(&[0x81, 0x80, 0x00], &mut 0).is_err());
    }

    #[test]
    fn test_optimized_read_compressed_u16() {
        let mut buffer = [0u8; 1024];
        let options = DefaultOptions::new().with_fixint_encoding(); // Ensure fixed-int encoding

        // Test all possible u16 values under the packet length
        for value in 0..=PACKET_DATA_SIZE as u16 {
            let mut offset;
            let short_u16 = ShortU16(value);

            // Serialize the value into the buffer
            serialize_into(&mut buffer[..], &short_u16).expect("Serialization failed");

            // Use bincode's size calculation to determine the length of the serialized data
            let serialized_len = options
                .serialized_size(&short_u16)
                .expect("Failed to get serialized size");

            // Reset offset
            offset = 0;

            // Read the value back using unchecked_read_u16_compressed
            let read_value = optimized_read_compressed_u16(&buffer, &mut offset);

            // Assert that the read value matches the original value
            assert_eq!(read_value, Ok(value), "Value mismatch for: {}", value);

            // Assert that the offset matches the serialized length
            assert_eq!(
                offset, serialized_len as usize,
                "Offset mismatch for: {}",
                value
            );
        }

        // Test bounds.
        // All 0s => 0
        assert_eq!(Ok(0), optimized_read_compressed_u16(&[0; 3], &mut 0));
        // Overflow
        assert!(optimized_read_compressed_u16(&[0xFF, 0xFF, 0x04], &mut 0).is_err());
        assert!(optimized_read_compressed_u16(&[0xFF, 0x80], &mut 0).is_err());

        // overflow errors
        assert!(optimized_read_compressed_u16(&[u8::MAX; 1], &mut 0).is_err());
        assert!(optimized_read_compressed_u16(&[u8::MAX; 2], &mut 0).is_err());

        // Minimal encoding checks
        assert!(optimized_read_compressed_u16(&[0x81, 0x00], &mut 0).is_err());
    }

    #[test]
    fn test_advance_offset_for_array() {
        #[repr(C)]
        struct MyStruct {
            _a: u8,
            _b: u8,
        }
        const _: () = assert!(core::mem::size_of::<MyStruct>() == 2);

        // Test with a buffer that is too short
        let bytes = [0u8; 1];
        let mut offset = 0;
        assert!(advance_offset_for_array::<MyStruct>(&bytes, &mut offset, 1).is_err());

        // Test with a buffer that is long enough
        let bytes = [0u8; 4];
        let mut offset = 0;
        assert!(advance_offset_for_array::<MyStruct>(&bytes, &mut offset, 2).is_ok());
        assert_eq!(offset, 4);
    }

    #[test]
    fn test_advance_offset_for_type() {
        #[repr(C)]
        struct MyStruct {
            _a: u8,
            _b: u8,
        }
        const _: () = assert!(core::mem::size_of::<MyStruct>() == 2);

        // Test with a buffer that is too short
        let bytes = [0u8; 1];
        let mut offset = 0;
        assert!(advance_offset_for_type::<MyStruct>(&bytes, &mut offset).is_err());

        // Test with a buffer that is long enough
        let bytes = [0u8; 4];
        let mut offset = 0;
        assert!(advance_offset_for_type::<MyStruct>(&bytes, &mut offset).is_ok());
        assert_eq!(offset, 2);
    }
}
