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
            Field::F32(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            Field::U8(v) => v.clone(),
            Field::U16(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            Field::U32(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            Field::I32(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
        }
    }
    pub fn from_bytes(type_name: &str, b: &[u8]) -> Option<Field> {
        Some(match type_name {
            "f32" => Field::F32(b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()),
            "u8" => Field::U8(b.to_vec()),
            "u16" => Field::U16(b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()),
            "u32" => Field::U32(b.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()),
            "i32" => Field::I32(b.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()),
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
        crate::hash::bytes(&self.to_bytes())
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
