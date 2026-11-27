//! Named per-cell arrays. Every stage writes plain data into a `Fields` map, so
//! any stage's output can be saved, inspected, or replaced by an imported file.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    F32(Vec<f32>),
    U8(Vec<u8>),
    U16(Vec<u16>),
    U32(Vec<u32>),
    I32(Vec<i32>),
}

impl Field {
    pub fn type_name(&self) -> &'static str {
        match self {
            Field::F32(_) => "f32",
            Field::U8(_) => "u8",
            Field::U16(_) => "u16",
            Field::U32(_) => "u32",
            Field::I32(_) => "i32",
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Field::F32(v) => v.len(),
            Field::U8(v) => v.len(),
            Field::U16(v) => v.len(),
            Field::U32(v) => v.len(),
            Field::I32(v) => v.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Little-endian raw bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Field::F32(v) => {
                let mut out = Vec::with_capacity(v.len() * 4);
                for x in v {
                    out.extend_from_slice(&x.to_le_bytes());
                }
                out
            }
            Field::U8(v) => v.clone(),
            Field::U16(v) => {
                let mut out = Vec::with_capacity(v.len() * 2);
                for x in v {
                    out.extend_from_slice(&x.to_le_bytes());
                }
                out
            }
            Field::U32(v) => {
                let mut out = Vec::with_capacity(v.len() * 4);
                for x in v {
                    out.extend_from_slice(&x.to_le_bytes());
                }
                out
            }
            Field::I32(v) => {
                let mut out = Vec::with_capacity(v.len() * 4);
                for x in v {
                    out.extend_from_slice(&x.to_le_bytes());
                }
                out
            }
        }
    }
    pub fn from_bytes(type_name: &str, b: &[u8]) -> Option<Field> {
        Some(match type_name {
            "f32" => {
                let (chunks, rem) = b.as_chunks::<4>();
                if !rem.is_empty() {
                    return None;
                }
                Field::F32(chunks.iter().map(|&c| f32::from_le_bytes(c)).collect())
            }
            "u8" => Field::U8(b.to_vec()),
            "u16" => {
                let (chunks, rem) = b.as_chunks::<2>();
                if !rem.is_empty() {
                    return None;
                }
                Field::U16(chunks.iter().map(|&c| u16::from_le_bytes(c)).collect())
            }
            "u32" => {
                let (chunks, rem) = b.as_chunks::<4>();
                if !rem.is_empty() {
                    return None;
                }
                Field::U32(chunks.iter().map(|&c| u32::from_le_bytes(c)).collect())
            }
            "i32" => {
                let (chunks, rem) = b.as_chunks::<4>();
                if !rem.is_empty() {
                    return None;
                }
                Field::I32(chunks.iter().map(|&c| i32::from_le_bytes(c)).collect())
            }
            _ => return None,
        })
    }
    /// Values as f32, whatever the storage type (for generic display).
    pub fn as_f32(&self) -> Vec<f32> {
        match self {
            Field::F32(v) => v.clone(),
            Field::U8(v) => v.iter().map(|&x| x as f32).collect(),
            Field::U16(v) => v.iter().map(|&x| x as f32).collect(),
            Field::U32(v) => v.iter().map(|&x| x as f32).collect(),
            Field::I32(v) => v.iter().map(|&x| x as f32).collect(),
        }
    }
    pub fn content_hash(&self) -> u64 {
        let mut h = crate::hash::StreamHasher::new();
        match self {
            Field::F32(v) => {
                for x in v {
                    h.write(&x.to_le_bytes());
                }
            }
            Field::U8(v) => h.write(v),
            Field::U16(v) => {
                for x in v {
                    h.write(&x.to_le_bytes());
                }
            }
            Field::U32(v) => {
                for x in v {
                    h.write(&x.to_le_bytes());
                }
            }
            Field::I32(v) => {
                for x in v {
                    h.write(&x.to_le_bytes());
                }
            }
        }
        h.finish()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fields(pub BTreeMap<String, Field>);

impl Fields {
    pub fn put(&mut self, name: &str, f: Field) {
        self.0.insert(name.to_string(), f);
    }
    pub fn get(&self, name: &str) -> Option<&Field> {
        self.0.get(name)
    }
    pub fn f32(&self, name: &str) -> &[f32] {
        match self.0.get(name) {
            Some(Field::F32(v)) => v,
            _ => panic!("missing f32 field {name}"),
        }
    }
    pub fn u8(&self, name: &str) -> &[u8] {
        match self.0.get(name) {
            Some(Field::U8(v)) => v,
            _ => panic!("missing u8 field {name}"),
        }
    }
    pub fn u16(&self, name: &str) -> &[u16] {
        match self.0.get(name) {
            Some(Field::U16(v)) => v,
            _ => panic!("missing u16 field {name}"),
        }
    }
    pub fn u32(&self, name: &str) -> &[u32] {
        match self.0.get(name) {
            Some(Field::U32(v)) => v,
            _ => panic!("missing u32 field {name}"),
        }
    }
    pub fn i32(&self, name: &str) -> &[i32] {
        match self.0.get(name) {
            Some(Field::I32(v)) => v,
            _ => panic!("missing i32 field {name}"),
        }
    }
    pub fn names(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }
}

/// Monthly fields are stored as 12 consecutive blocks of `n` values.
pub fn month_slice(v: &[f32], n: usize, m: usize) -> &[f32] {
    &v[m * n..(m + 1) * n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_matches_bytes_and_rejects_unaligned() {
        let samples = [
            Field::F32(vec![1.5, -2.25, 0.0, 42.125, 9.5]),
            Field::U8(vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
            Field::U16(vec![10, 20, 30, 65000, 7]),
            Field::U32(vec![1, 2, 3, 0xDEAD_BEEF, 99]),
            Field::I32(vec![-1, 0, 1, i32::MIN, i32::MAX]),
        ];
        for f in &samples {
            let bytes = f.to_bytes();
            assert_eq!(f.content_hash(), crate::hash::bytes(&bytes));
            assert_eq!(Field::from_bytes(f.type_name(), &bytes).as_ref(), Some(f));
        }
        assert!(Field::from_bytes("f32", &[0, 1, 2]).is_none());
        assert!(Field::from_bytes("u16", &[0, 1, 2]).is_none());
        assert!(Field::from_bytes("u32", &[0, 1, 2, 3, 4]).is_none());
        assert!(Field::from_bytes("i32", &[0]).is_none());
    }
}
