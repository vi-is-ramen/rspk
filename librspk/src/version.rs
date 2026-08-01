//! Representations of the librspk version.

const fn parse_usize(s: &str) -> usize
{
    let bytes = s.as_bytes();
    let mut result = 0usize;
    let mut i = 0;

    while i < bytes.len()
    {
        let byte = bytes[i];

        if byte < b'0' || byte > b'9'
        {
            panic!("Invalid digit in string");
        }

        let digit = (byte - b'0') as usize;

        if result > (usize::MAX - digit) / 10
        {
            panic!("Overflow while parsing usize");
        }

        result = result * 10 + digit;
        i += 1;
    }

    result
}

const fn fnv1a_64(data: &[u8]) -> u64
{
    let mut hash = 0xcbf29ce484222325u64; // offset basis
    let mut i = 0;

    while i < data.len()
    {
        hash ^= data[i] as u64;
        hash = hash.wrapping_mul(0x00000100000001b3u64); // FNV prime
        i += 1;
    }

    hash
}

/// String representation of the **librspk** version.
pub const STRING: &str = env!["CARGO_PKG_VERSION"];

/// Hash of the **librspk** version.
pub const HASH: u64 = fnv1a_64(STRING.as_bytes());

/// Tuple representation of the **librspk** version (M, m, p).
pub const TUPLE: (usize, usize, usize) = (
    parse_usize(env!["CARGO_PKG_VERSION_MAJOR"]),
    parse_usize(env!["CARGO_PKG_VERSION_MINOR"]),
    parse_usize(env!["CARGO_PKG_VERSION_PATCH"]),
);

/// Slice representation of the **librspk** version (M, m, p).
pub const SLICE: [usize; 3] = [
    parse_usize(env!["CARGO_PKG_VERSION_MAJOR"]),
    parse_usize(env!["CARGO_PKG_VERSION_MINOR"]),
    parse_usize(env!["CARGO_PKG_VERSION_PATCH"]),
];
