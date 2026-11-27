//! Stable 64-bit content hashing (FNV-1a + final mix). Used for stage cache keys,
//! so it must never change between versions or platforms.

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

/// Streaming equivalent of [`bytes`], producing bit-for-bit identical digests
/// without materialising an intermediate `Vec<u8>`.
pub struct StreamHasher {
    h: u64,
    len: u64,
    buf: [u8; 8],
    buf_len: usize,
}

impl Default for StreamHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamHasher {
    pub fn new() -> Self {
        Self { h: FNV_OFFSET, len: 0, buf: [0; 8], buf_len: 0 }
    }

    #[inline]
    pub fn write(&mut self, mut b: &[u8]) {
        self.len += b.len() as u64;
        if self.buf_len > 0 {
            let take = (8 - self.buf_len).min(b.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&b[..take]);
            self.buf_len += take;
            b = &b[take..];
            if self.buf_len == 8 {
                let v = u64::from_le_bytes(self.buf);
                self.h ^= v;
                self.h = self.h.wrapping_mul(FNV_PRIME).rotate_left(29);
                self.buf_len = 0;
            }
        }
        let (chunks, rem) = b.as_chunks::<8>();
        for &c in chunks {
            let v = u64::from_le_bytes(c);
            self.h ^= v;
            self.h = self.h.wrapping_mul(FNV_PRIME).rotate_left(29);
        }
        if !rem.is_empty() {
            self.buf[..rem.len()].copy_from_slice(rem);
            self.buf_len = rem.len();
        }
    }

    pub fn finish(mut self) -> u64 {
        for &x in &self.buf[..self.buf_len] {
            self.h ^= x as u64;
            self.h = self.h.wrapping_mul(FNV_PRIME);
        }
        crate::rng::mix64(self.h ^ self.len)
    }
}

pub fn bytes(b: &[u8]) -> u64 {
    let mut h: u64 = FNV_OFFSET;
    // Process 8 bytes at a time for speed, then the tail.
    let (chunks, rem) = b.as_chunks::<8>();
    for &c in chunks {
        let v = u64::from_le_bytes(c);
        h ^= v;
        h = h.wrapping_mul(FNV_PRIME).rotate_left(29);
    }
    for &x in rem {
        h ^= x as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    crate::rng::mix64(h ^ b.len() as u64)
}

pub fn combine(a: u64, b: u64) -> u64 {
    crate::rng::mix64(a ^ b.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17))
}

pub fn json<T: serde::Serialize>(v: &T) -> u64 {
    let s = serde_json::to_string(v).expect("world parameters and edits must be JSON-serializable");
    bytes(s.as_bytes())
}

pub fn hex(h: u64) -> String {
    format!("{h:016x}")
}
