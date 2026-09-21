//! Bounded, explicit Russian letter-name heuristic; not a phonetic model.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Pronunciation {
    RuLetters,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AliasOrigin {
    Manual,
    Generated,
    ManualAndGenerated,
}

impl Pronunciation {
    pub fn generate(self, canonical: &str) -> Result<Vec<String>> {
        let letters: Vec<_> = canonical.chars().collect();
        ensure!(
            (2..=3).contains(&letters.len()),
            "ru_letters requires 2..3 uppercase Russian letters"
        );
        let mut variants = vec![String::new()];
        for letter in letters {
            let names: &[&str] = match letter {
                'А' => &["а"],
                'Б' => &["бэ"],
                'В' => &["вэ"],
                'Г' => &["гэ"],
                'Д' => &["дэ"],
                'Е' => &["е"],
                'Ё' => &["ё"],
                'Ж' => &["жэ"],
                'З' => &["зэ"],
                'И' => &["и"],
                'К' => &["ка"],
                'Л' => &["эль", "эл"],
                'М' => &["эм"],
                'Н' => &["эн"],
                'О' => &["о"],
                'П' => &["пэ"],
                'Р' => &["эр"],
                'С' => &["эс"],
                'Т' => &["тэ"],
                'У' => &["у"],
                'Ф' => &["эф"],
                'Х' => &["ха"],
                'Ц' => &["цэ"],
                'Ч' => &["чэ"],
                'Ш' => &["ша"],
                'Щ' => &["ща"],
                'Э' => &["э"],
                'Ю' => &["ю"],
                'Я' => &["я"],
                _ => bail!("unsupported ru_letters character: {letter}; use manual aliases"),
            };
            let mut next = Vec::new();
            for prefix in &variants {
                for name in names {
                    next.push(format!("{prefix}{name}"));
                    if !prefix.is_empty() {
                        next.push(format!("{prefix} {name}"));
                    }
                }
            }
            ensure!(next.len() <= 32, "too many generated pronunciations");
            variants = next;
        }
        Ok(variants
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }
}
