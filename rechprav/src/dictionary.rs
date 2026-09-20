//! Unicode Levenshtein lookup with trie/DP pruning. No full dictionary scan per query.
use crate::text::{key, tokenize};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::UnicodeNormalization;

/// Dictionary membership and candidate generation, independent of correction policy.
pub trait CandidateGenerator {
    fn contains(&self, word: &str) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn node_count(&self) -> usize;
    fn lookup(
        &self,
        query: &str,
        distance: usize,
        max_candidates: usize,
        max_nodes: usize,
    ) -> Search;
    fn storage_metadata(&self) -> serde_json::Value;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DictionaryFile {
    pub schema_version: u32,
    pub strict_yo: bool,
    pub words: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub word: String,
    pub key: String,
    pub distance: usize,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_per_billion: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Search {
    pub candidates: Vec<Candidate>,
    pub complete: bool,
    pub visited_nodes: usize,
    pub stop_reason: Option<&'static str>,
}

#[derive(Default)]
struct Node {
    children: BTreeMap<char, usize>,
    terminal: Option<usize>,
}

pub struct Dictionary {
    words: BTreeMap<String, String>,
    entries: Vec<(String, String)>,
    nodes: Vec<Node>,
    strict_yo: bool,
}

pub fn word_lines(contents: &str) -> Vec<&str> {
    contents
        .strip_prefix('\u{feff}')
        .unwrap_or(contents)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

pub fn protected_words(contents: &str, strict_yo: bool) -> Result<BTreeSet<String>> {
    word_lines(contents)
        .into_iter()
        .map(|word| {
            let tokens = tokenize(word, strict_yo);
            ensure!(
                tokens.lexical.len() == 1
                    && tokens.lexical[0].byte_start == 0
                    && tokens.lexical[0].byte_end == word.len(),
                "domain/ignore entry must be a single lexical atom: {word}"
            );
            Ok(key(word, strict_yo))
        })
        .collect()
}

impl Dictionary {
    /// Offline conversion only: breadth-first nodes and a single canonical spelling blob.
    pub fn pack(&self, source_sha256: [u8; 32]) -> Result<Vec<u8>> {
        use crate::packed::{PackedNode, encode, label};
        ensure!(self.nodes.len() <= u32::MAX as usize, "too many trie nodes");
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut rows = Vec::with_capacity(self.nodes.len());
        let mut blob = Vec::new();
        order.push(0usize);
        rows.push(PackedNode {
            label: 255,
            ..Default::default()
        });
        let mut cursor = 0;
        while cursor < order.len() {
            let node = &self.nodes[order[cursor]];
            if !node.children.is_empty() {
                rows[cursor].first_child = order.len() as u32;
                rows[cursor].child_count = node.children.len() as u8;
            }
            if let Some(id) = node.terminal {
                let spelling = &self.entries[id].1;
                rows[cursor].word_start = u32::try_from(blob.len())?;
                rows[cursor].word_len = spelling.len() as u16;
                blob.extend_from_slice(spelling.as_bytes());
                ensure!(
                    blob.len() <= u32::MAX as usize,
                    "spelling blob exceeds 4 GiB"
                );
            }
            for (&ch, &child) in &node.children {
                order.push(child);
                rows.push(PackedNode {
                    label: label(ch).unwrap(),
                    ..Default::default()
                });
            }
            cursor += 1;
        }
        encode(self.strict_yo, self.len(), source_sha256, &rows, &blob)
    }
    pub fn from_text(contents: &str, strict_yo: bool) -> Result<Self> {
        Self::from_words(
            word_lines(contents).into_iter().map(str::to_owned),
            strict_yo,
        )
    }

    pub fn from_json(contents: &str, strict_yo: bool) -> Result<Self> {
        let file: DictionaryFile = serde_json::from_str(contents)?;
        ensure!(file.schema_version == 1, "unsupported dictionary schema");
        ensure!(
            file.strict_yo == strict_yo,
            "dictionary/config strict_yo mismatch; rebuild dictionary"
        );
        Self::from_words(file.words, strict_yo)
    }

    pub fn from_words(words: impl IntoIterator<Item = String>, strict_yo: bool) -> Result<Self> {
        let mut canonical: BTreeMap<String, String> = BTreeMap::new();
        for word in words {
            let spelling: String = word.nfc().flat_map(char::to_lowercase).nfc().collect();
            ensure!(
                !spelling.is_empty()
                    && spelling.chars().count() <= 128
                    && spelling
                        .chars()
                        .all(|c| ('а'..='я').contains(&c) || c == 'ё'),
                "main dictionary expects Russian word forms of 1..128 letters: {word}"
            );
            let normalized = key(&spelling, strict_yo);
            canonical
                .entry(normalized.clone())
                .and_modify(|old| {
                    // Preserve ё when it is the only spelling, otherwise prefer the normalized one.
                    if spelling == normalized || (*old != normalized && spelling < *old) {
                        *old = spelling.clone();
                    }
                })
                .or_insert(spelling);
        }
        ensure!(!canonical.is_empty(), "main dictionary is empty");
        let entries: Vec<_> = canonical
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut nodes = vec![Node::default()];
        for (word_id, (word, _)) in entries.iter().enumerate() {
            let mut node = 0;
            for ch in word.chars() {
                let next = if let Some(&child) = nodes[node].children.get(&ch) {
                    child
                } else {
                    let index = nodes.len();
                    nodes.push(Node::default());
                    nodes[node].children.insert(ch, index);
                    index
                };
                node = next;
            }
            nodes[node].terminal = Some(word_id);
        }
        Ok(Self {
            words: canonical,
            entries,
            nodes,
            strict_yo,
        })
    }

    pub fn contains(&self, word: &str) -> bool {
        self.words.contains_key(word)
    }
    pub fn len(&self) -> usize {
        self.words.len()
    }
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn export(&self) -> DictionaryFile {
        DictionaryFile {
            schema_version: 1,
            strict_yo: self.strict_yo,
            words: self.words.values().cloned().collect(),
        }
    }

    pub fn lookup(
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
        let mut stack = vec![(0, (0..=query.len()).collect::<Vec<_>>())];
        while let Some((node, row)) = stack.pop() {
            if result.visited_nodes >= max_nodes {
                result.complete = false;
                result.stop_reason = Some("search_node_limit");
                break;
            }
            result.visited_nodes += 1;
            if row[query.len()] <= distance
                && let Some(id) = self.nodes[node].terminal
            {
                let (key, word) = &self.entries[id];
                result.candidates.push(Candidate {
                    key: key.clone(),
                    word: word.clone(),
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
            for (&ch, &child) in self.nodes[node].children.iter().rev() {
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

impl CandidateGenerator for Dictionary {
    fn contains(&self, word: &str) -> bool {
        self.contains(word)
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn node_count(&self) -> usize {
        self.node_count()
    }
    fn lookup(
        &self,
        query: &str,
        distance: usize,
        max_candidates: usize,
        max_nodes: usize,
    ) -> Search {
        self.lookup(query, distance, max_candidates, max_nodes)
    }
    fn storage_metadata(&self) -> serde_json::Value {
        serde_json::json!({"format":"btree_trie"})
    }
}
