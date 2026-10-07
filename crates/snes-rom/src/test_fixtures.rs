//! Byte fixtures shared by unit tests.

/// The vanilla SMW (USA) internal header bytes (`$00:FFC0-$00:FFDF`).
pub const SMW_HEADER: [u8; 32] = [
    b'S', b'U', b'P', b'E', b'R', b' ', b'M', b'A', b'R', b'I', b'O', b'W', b'O', b'R', b'L', b'D',
    b' ', b' ', b' ', b' ', b' ', 0x20, 0x02, 0x09, 0x01, 0x01, 0x01, 0x00, 0x25, 0x5F, 0xDA, 0xA0,
];
