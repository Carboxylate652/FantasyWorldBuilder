//! Stable 64-bit content hashing (FNV-1a + final mix). Used for stage cache keys,
//! so it must never change between versions or platforms.

pub fn bytes(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    // Process 8 bytes at a time for speed, then the tail.
    let mut chunks = b.chunks_exact(8);
    for c in &mut chunks {
        let v = u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]);
        h ^= v;
        h = h.wrapping_mul(0x0000_0100_0000_01B3).rotate_left(29);
    }
    for &x in chunks.remainder() {
        h ^= x as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    crate::rng::mix64(h ^ b.len() as u64)
}

pub fn combine(a: u64, b: u64) -> u64 {
    crate::rng::mix64(a ^ b.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17))
}

pub fn json<T: serde::Serialize>(v: &T) -> u64 {
    bytes(serde_json::to_string(v).unwrap_or_default().as_bytes())
}

pub fn hex(h: u64) -> String {
    format!("{h:016x}")
}
