//! Validate an immutable publication bundle by replaying the producer, without writes.
#[path = "../src/feedback.rs"]
mod feedback;
#[path = "../src/feedback_composition.rs"]
mod feedback_composition;
#[path = "../src/feedback_registry.rs"]
mod feedback_registry;
#[path = "../src/profile_validation.rs"]
mod profile_validation;
use anyhow::Result;
use std::io::{self, Read};
fn main() -> Result<()> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    let (_, metadata) = profile_validation::validate(&bytes)?;
    println!("{metadata}");
    Ok(())
}
