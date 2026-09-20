//! Sorted UTF-8 keys and exact IEEE-754 f64 values, validated before binary search.
use crate::hashing::hex;
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MAGIC: &[u8; 8] = b"RCHFQ001";
const HEADER: usize = 96;
const ROW: usize = 16;

fn checksum(data: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(&data[..64]);
    hash.update(&data[HEADER..]);
    hash.finalize().into()
}

pub(crate) fn encode(
    values: &BTreeMap<String, f64>,
    strict_yo: bool,
    source: [u8; 32],
) -> Result<Vec<u8>> {
    ensure!(
        !values.is_empty() && values.len() <= u32::MAX as usize,
        "invalid frequency count"
    );
    let blob_len = values
        .keys()
        .try_fold(0usize, |size, key| size.checked_add(key.len()))
        .ok_or_else(|| anyhow::anyhow!("frequency blob overflow"))?;
    ensure!(
        blob_len <= u32::MAX as usize,
        "frequency blob exceeds 4 GiB"
    );
    let size = values
        .len()
        .checked_mul(ROW)
        .and_then(|v| v.checked_add(HEADER))
        .and_then(|v| v.checked_add(blob_len))
        .ok_or_else(|| anyhow::anyhow!("frequency file size overflow"))?;
    let mut data = Vec::with_capacity(size);
    data.extend_from_slice(MAGIC);
    data.push(u8::from(strict_yo));
    data.extend_from_slice(&[0; 7]);
    data.extend_from_slice(&(values.len() as u64).to_le_bytes());
    data.extend_from_slice(&(blob_len as u64).to_le_bytes());
    data.extend_from_slice(&source);
    data.extend_from_slice(&[0; 32]);
    let mut offset = 0u32;
    for (key, value) in values {
        data.extend_from_slice(&offset.to_le_bytes());
        data.extend_from_slice(&(key.len() as u16).to_le_bytes());
        data.extend_from_slice(&[0; 2]);
        data.extend_from_slice(&value.to_bits().to_le_bytes());
        offset += key.len() as u32;
    }
    for key in values.keys() {
        data.extend_from_slice(key.as_bytes());
    }
    let digest = checksum(&data);
    data[64..HEADER].copy_from_slice(&digest);
    Ok(data)
}

pub struct PackedFrequency {
    data: Vec<u8>,
    count: usize,
    blob_start: usize,
}

impl PackedFrequency {
    pub fn from_bytes(data: Vec<u8>, strict_yo: bool) -> Result<Self> {
        ensure!(
            data.len() >= HEADER && &data[..8] == MAGIC,
            "invalid frequency magic/version/header"
        );
        ensure!(
            data[8] <= 1 && (data[8] == 1) == strict_yo,
            "frequency/config strict_yo mismatch"
        );
        ensure!(
            data[9..16].iter().all(|&v| v == 0),
            "invalid frequency header reserved bytes"
        );
        let count = usize::try_from(u64::from_le_bytes(data[16..24].try_into().unwrap()))?;
        let blob_len = usize::try_from(u64::from_le_bytes(data[24..32].try_into().unwrap()))?;
        ensure!(
            count > 0 && count <= u32::MAX as usize && blob_len <= u32::MAX as usize,
            "invalid frequency sizes"
        );
        let blob_start = count
            .checked_mul(ROW)
            .and_then(|v| v.checked_add(HEADER))
            .ok_or_else(|| anyhow::anyhow!("frequency table size overflow"))?;
        ensure!(
            blob_start.checked_add(blob_len) == Some(data.len()),
            "frequency size mismatch/trailing data"
        );
        ensure!(
            checksum(&data) == data[64..HEADER],
            "frequency checksum mismatch"
        );
        let table = Self {
            data,
            count,
            blob_start,
        };
        let mut cursor = 0usize;
        let mut previous = None;
        for index in 0..count {
            let row = table.row(index);
            let offset = u32::from_le_bytes(row[..4].try_into().unwrap()) as usize;
            let length = u16::from_le_bytes(row[4..6].try_into().unwrap()) as usize;
            ensure!(
                offset == cursor
                    && (2..=256).contains(&length)
                    && length.is_multiple_of(2)
                    && row[6..8] == [0, 0],
                "invalid frequency row offset/length/reserved bytes"
            );
            cursor = cursor
                .checked_add(length)
                .ok_or_else(|| anyhow::anyhow!("frequency offset overflow"))?;
            ensure!(cursor <= blob_len, "frequency key outside blob");
            let key = std::str::from_utf8(&table.data[blob_start + offset..blob_start + cursor])?;
            ensure!(
                key.chars()
                    .all(|c| ('а'..='я').contains(&c) || (strict_yo && c == 'ё')),
                "invalid/unnormalized frequency key"
            );
            ensure!(
                previous.is_none_or(|p| p < key),
                "frequency keys are not strictly sorted/unique"
            );
            let value = table.value(index);
            ensure!(
                value.is_finite() && (0.0..=1e9).contains(&value),
                "invalid frequency value"
            );
            previous = Some(key);
        }
        ensure!(cursor == blob_len, "unreferenced frequency bytes");
        Ok(table)
    }
    fn row(&self, index: usize) -> &[u8] {
        &self.data[HEADER + index * ROW..HEADER + (index + 1) * ROW]
    }
    fn key(&self, index: usize) -> &str {
        let row = self.row(index);
        let offset = u32::from_le_bytes(row[..4].try_into().unwrap()) as usize;
        let length = u16::from_le_bytes(row[4..6].try_into().unwrap()) as usize;
        std::str::from_utf8(&self.data[self.blob_start + offset..self.blob_start + offset + length])
            .unwrap()
    }
    fn value(&self, index: usize) -> f64 {
        f64::from_bits(u64::from_le_bytes(
            self.row(index)[8..16].try_into().unwrap(),
        ))
    }
    pub fn get(&self, key: &str) -> Option<f64> {
        let (mut low, mut high) = (0, self.count);
        while low < high {
            let middle = low + (high - low) / 2;
            match self.key(middle).cmp(key) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return Some(self.value(middle)),
            }
        }
        None
    }
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
    pub fn source_sha256(&self) -> String {
        hex(&self.data[32..64])
    }
    pub fn entries(&self) -> impl Iterator<Item = (&str, f64)> {
        (0..self.count).map(|index| (self.key(index), self.value(index)))
    }
}
