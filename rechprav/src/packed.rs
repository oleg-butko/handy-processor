//! Portable, validated, read-only trie. No unsafe casts or native-layout serialization.
//! Header: magic[8], strict_yo[1], reserved[7], node_count/word_count/blob_len u64 LE,
//! source SHA256[32], checksum[32]. Checksum covers header[0..72] and payload.
//! Payload: 12-byte BFS nodes, then concatenated canonical UTF-8 spellings.
use crate::{
    dictionary::{Candidate, CandidateGenerator, Search},
    text::key,
};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 8] = b"RCHPTR01";
const HEADER: usize = 104;
const NODE: usize = 12;

#[derive(Default, Clone, Copy)]
pub(crate) struct PackedNode {
    pub first_child: u32,
    pub word_start: u32,
    pub word_len: u16,
    pub child_count: u8,
    pub label: u8,
}

pub(crate) fn label(ch: char) -> Option<u8> {
    match ch {
        'а'..='я' => Some((ch as u32 - 'а' as u32) as u8),
        'ё' => Some(32),
        _ => None,
    }
}

fn letter(label: u8) -> char {
    if label == 32 {
        'ё'
    } else {
        char::from_u32('а' as u32 + u32::from(label)).unwrap()
    }
}

fn checksum(data: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(&data[..72]);
    hash.update(&data[HEADER..]);
    hash.finalize().into()
}

pub(crate) fn encode(
    strict_yo: bool,
    words: usize,
    source: [u8; 32],
    rows: &[PackedNode],
    blob: &[u8],
) -> Result<Vec<u8>> {
    let size = rows
        .len()
        .checked_mul(NODE)
        .and_then(|v| v.checked_add(HEADER))
        .and_then(|v| v.checked_add(blob.len()))
        .ok_or_else(|| anyhow::anyhow!("index size overflow"))?;
    let mut data = Vec::with_capacity(size);
    data.extend_from_slice(MAGIC);
    data.push(u8::from(strict_yo));
    data.extend_from_slice(&[0; 7]);
    for value in [rows.len(), words, blob.len()] {
        data.extend_from_slice(&(value as u64).to_le_bytes());
    }
    data.extend_from_slice(&source);
    data.extend_from_slice(&[0; 32]);
    for node in rows {
        data.extend_from_slice(&node.first_child.to_le_bytes());
        data.extend_from_slice(&node.word_start.to_le_bytes());
        data.extend_from_slice(&node.word_len.to_le_bytes());
        data.push(node.child_count);
        data.push(node.label);
    }
    data.extend_from_slice(blob);
    let digest = checksum(&data);
    data[72..HEADER].copy_from_slice(&digest);
    Ok(data)
}

pub struct PackedTrie {
    data: Vec<u8>,
    nodes: usize,
    words: usize,
    blob_start: usize,
    strict_yo: bool,
}

impl PackedTrie {
    pub fn from_bytes(data: Vec<u8>, strict_yo: bool) -> Result<Self> {
        ensure!(
            data.len() >= HEADER && &data[..8] == MAGIC,
            "invalid packed trie magic/version/header"
        );
        ensure!(
            data[8] <= 1 && (data[8] == 1) == strict_yo,
            "packed trie/config strict_yo mismatch"
        );
        ensure!(
            data[9..16].iter().all(|&b| b == 0),
            "invalid reserved header bytes"
        );
        let count = |offset| -> Result<usize> {
            Ok(usize::try_from(u64::from_le_bytes(
                data[offset..offset + 8].try_into().unwrap(),
            ))?)
        };
        let nodes = count(16)?;
        let words = count(24)?;
        let blob_len = count(32)?;
        ensure!(
            nodes > 1
                && nodes <= u32::MAX as usize
                && words > 0
                && words < nodes
                && blob_len <= u32::MAX as usize,
            "invalid packed trie counts"
        );
        let blob_start = nodes
            .checked_mul(NODE)
            .and_then(|v| v.checked_add(HEADER))
            .ok_or_else(|| anyhow::anyhow!("packed trie size overflow"))?;
        ensure!(
            blob_start.checked_add(blob_len) == Some(data.len()),
            "packed trie size mismatch/trailing data"
        );
        ensure!(
            checksum(&data) == data[72..HEADER],
            "packed trie checksum mismatch"
        );
        // Check the entire blob before taking UTF-8 slices from terminal offsets.
        let blob = std::str::from_utf8(&data[blob_start..])?;
        ensure!(
            blob.chars().all(|ch| label(ch).is_some()),
            "non-Russian spelling blob"
        );
        let trie = Self {
            data,
            nodes,
            words,
            blob_start,
            strict_yo,
        };
        trie.validate_structure()?;
        Ok(trie)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
    pub fn source_sha256(&self) -> String {
        self.data[40..72]
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect()
    }
    fn node(&self, index: usize) -> PackedNode {
        let offset = HEADER + NODE * index;
        let row = &self.data[offset..offset + NODE];
        PackedNode {
            first_child: u32::from_le_bytes(row[..4].try_into().unwrap()),
            word_start: u32::from_le_bytes(row[4..8].try_into().unwrap()),
            word_len: u16::from_le_bytes(row[8..10].try_into().unwrap()),
            child_count: row[10],
            label: row[11],
        }
    }
    fn spelling(&self, node: PackedNode) -> &str {
        let start = self.blob_start + node.word_start as usize;
        std::str::from_utf8(&self.data[start..start + node.word_len as usize]).unwrap()
    }
    fn validate_structure(&self) -> Result<()> {
        let root = self.node(0);
        ensure!(root.label == 255 && root.word_len == 0, "invalid root");
        let mut parents = vec![0u32; self.nodes];
        let (mut next_child, mut next_word, mut word_count) = (1usize, 0usize, 0usize);
        for index in 0..self.nodes {
            let node = self.node(index);
            if index > 0 {
                ensure!(
                    node.label <= 32 && (self.strict_yo || node.label != 32),
                    "invalid/unnormalized label"
                );
                ensure!(parents[index] < index as u32, "unreachable node or cycle");
            }
            ensure!(node.child_count <= 33, "too many children");
            if node.child_count == 0 {
                ensure!(node.first_child == 0 && node.word_len > 0, "invalid leaf");
            } else {
                let start = node.first_child as usize;
                let end = start
                    .checked_add(node.child_count as usize)
                    .ok_or_else(|| anyhow::anyhow!("child range overflow"))?;
                ensure!(
                    start == next_child && start > index && end <= self.nodes,
                    "invalid BFS child range"
                );
                let mut previous = None;
                for (child, parent) in parents.iter_mut().enumerate().take(end).skip(start) {
                    let child_label = self.node(child).label;
                    ensure!(
                        previous.is_none_or(|p| p < child_label),
                        "unsorted/duplicate child label"
                    );
                    *parent = index as u32;
                    previous = Some(child_label);
                }
                next_child = end;
            }
            if node.word_len == 0 {
                ensure!(node.word_start == 0, "nonterminal has spelling offset");
                continue;
            }
            let length = node.word_len as usize;
            ensure!(
                length <= 256 && length.is_multiple_of(2) && node.word_start as usize == next_word,
                "invalid spelling offset/length"
            );
            next_word += length;
            ensure!(
                next_word <= self.data.len() - self.blob_start,
                "spelling outside blob"
            );
            // Every Russian character is two UTF-8 bytes, so the even boundaries are valid.
            let mut ancestor = index;
            for ch in self.spelling(node).chars().rev() {
                let normalized = if !self.strict_yo && ch == 'ё' {
                    'е'
                } else {
                    ch
                };
                ensure!(
                    ancestor > 0 && self.node(ancestor).label == label(normalized).unwrap(),
                    "canonical spelling does not match trie path"
                );
                ancestor = parents[ancestor] as usize;
            }
            ensure!(ancestor == 0, "canonical spelling/path length mismatch");
            word_count += 1;
        }
        ensure!(
            next_child == self.nodes
                && word_count == self.words
                && next_word == self.data.len() - self.blob_start,
            "unreachable or uncounted trie data"
        );
        Ok(())
    }
}

impl CandidateGenerator for PackedTrie {
    fn contains(&self, word: &str) -> bool {
        let mut index = 0;
        for ch in word.chars() {
            let Some(wanted) = label(ch) else {
                return false;
            };
            let node = self.node(index);
            let mut low = node.first_child as usize;
            let mut high = low + node.child_count as usize;
            let end = high;
            while low < high {
                let middle = low + (high - low) / 2;
                if self.node(middle).label < wanted {
                    low = middle + 1;
                } else {
                    high = middle;
                }
            }
            if low == end || self.node(low).label != wanted {
                return false;
            }
            index = low;
        }
        self.node(index).word_len > 0
    }
    fn len(&self) -> usize {
        self.words
    }
    fn node_count(&self) -> usize {
        self.nodes
    }
    fn storage_metadata(&self) -> serde_json::Value {
        serde_json::json!({"format":"packed_trie_v1", "bytes":self.data.len(), "source_sha256":self.source_sha256()})
    }
    fn lookup(
        &self,
        query: &str,
        distance: usize,
        max_candidates: usize,
        max_nodes: usize,
    ) -> Search {
        let query: Vec<_> = query.chars().collect();
        let mut result = Search {
            candidates: vec![],
            complete: true,
            visited_nodes: 0,
            stop_reason: None,
        };
        let mut stack = vec![(0usize, (0..=query.len()).collect::<Vec<_>>())];
        while let Some((index, row)) = stack.pop() {
            if result.visited_nodes >= max_nodes {
                result.complete = false;
                result.stop_reason = Some("search_node_limit");
                break;
            }
            result.visited_nodes += 1;
            let node = self.node(index);
            if row[query.len()] <= distance && node.word_len > 0 {
                let word = self.spelling(node);
                result.candidates.push(Candidate {
                    word: word.to_owned(),
                    key: key(word, self.strict_yo),
                    distance: row[query.len()],
                    score: -(row[query.len()] as f64),
                    frequency_per_billion: None,
                });
                if result.candidates.len() > max_candidates {
                    result.complete = false;
                    result.stop_reason = Some("candidate_limit");
                    break;
                }
            }
            let start = node.first_child as usize;
            for child in (start..start + node.child_count as usize).rev() {
                let ch = letter(self.node(child).label);
                let mut next = vec![row[0] + 1];
                for j in 0..query.len() {
                    next.push(
                        (next[j] + 1)
                            .min(row[j + 1] + 1)
                            .min(row[j] + usize::from(ch != query[j])),
                    );
                }
                if *next.iter().min().unwrap() <= distance {
                    stack.push((child, next));
                }
            }
        }
        result
            .candidates
            .sort_by(|a, b| (a.distance, &a.key, &a.word).cmp(&(b.distance, &b.key, &b.word)));
        result.candidates.truncate(max_candidates);
        result
    }
}
