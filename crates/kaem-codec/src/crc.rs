//! CRC-16/CCITT-FALSE (poly 0x1021, init 0xFFFF, no reflection, xorout 0x0000).
//!
//! Small, dependency-free, and good enough to catch the bit errors a noisy
//! radio link introduces.

pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        // CRC-16/CCITT-FALSE of "123456789" is 0x29B1.
        assert_eq!(crc16(b"123456789"), 0x29B1);
    }

    #[test]
    fn empty_is_init() {
        assert_eq!(crc16(b""), 0xFFFF);
    }
}
