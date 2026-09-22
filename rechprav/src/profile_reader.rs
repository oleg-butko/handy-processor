//! Single-thread ownership model. Runtime traits remain unchanged; no Send/Sync claim.
use crate::profile_validation;
use anyhow::{Context, Result, ensure};
use rechprav::{
    hashing::sha256_hex,
    terms::{Analysis, TermEngine},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::rc::Rc;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub scope: String,
    pub desired_revision: u64,
    pub active: Option<Published>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Published {
    pub generation: u64,
    pub request_id: String,
    pub bundle_sha256: String,
    pub bundle: Value,
    pub validation: Value,
    pub receipt: Value,
}
#[derive(Clone, PartialEq, Eq)]
struct Identity {
    generation: u64,
    request_id: String,
    bundle_sha256: String,
}
impl Published {
    fn identity(&self) -> Identity {
        Identity {
            generation: self.generation,
            request_id: self.request_id.clone(),
            bundle_sha256: self.bundle_sha256.clone(),
        }
    }
    fn check(&self, scope: &str, desired: u64) -> Result<Vec<u8>> {
        ensure!(
            self.generation > 0 && self.generation <= desired,
            "invalid active generation"
        );
        valid_id(&self.request_id)?;
        let bytes = serde_json::to_vec(&self.bundle)?;
        ensure!(
            bytes.len() <= 8 * 1024 * 1024 && sha256_hex(&bytes) == self.bundle_sha256,
            "bundle hash mismatch"
        );
        let config = self.bundle["config"]
            .as_str()
            .context("missing config text")?;
        let config_hash = sha256_hex(config.as_bytes());
        ensure!(
            self.validation["schema_version"] == 1
                && self.validation["scope"].as_str() == Some(scope)
                && self.validation["bundle_sha256"].as_str() == Some(&self.bundle_sha256)
                && self.validation["config_sha256"].as_str() == Some(&config_hash)
                && self.receipt["request_id"].as_str() == Some(&self.request_id)
                && self.receipt["generation"].as_u64() == Some(self.generation)
                && self.receipt["bundle_sha256"].as_str() == Some(&self.bundle_sha256)
                && self.receipt["config_sha256"].as_str() == Some(&config_hash),
            "snapshot receipt/validation mismatch"
        );
        Ok(bytes)
    }
}
pub fn valid_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "invalid id"
    );
    Ok(())
}
pub struct Ticket {
    scope: String,
    desired: u64,
    identity: Identity,
    bytes: Vec<u8>,
}
pub struct Candidate {
    desired: u64,
    scope: String,
    profile: Rc<LoadedProfile>,
}
pub struct LoadedProfile {
    identity: Identity,
    config_sha256: String,
    engine: TermEngine,
}
#[derive(Serialize)]
pub struct Output {
    pub generation: u64,
    pub request_id: String,
    pub config_sha256: String,
    pub analysis: Analysis,
}
impl LoadedProfile {
    pub fn analyze(&self, text: &str) -> Output {
        Output {
            generation: self.identity.generation,
            request_id: self.identity.request_id.clone(),
            config_sha256: self.config_sha256.clone(),
            analysis: self.engine.analyze(text),
        }
    }
}
pub fn prepare(ticket: Ticket) -> Result<Candidate> {
    // The engine contains the SAME Baseline bytes whose hashes replay validated.
    let (engine, metadata) = profile_validation::validate(&ticket.bytes)?;
    ensure!(
        metadata["scope"].as_str() == Some(&ticket.scope),
        "replayed scope mismatch"
    );
    Ok(Candidate {
        desired: ticket.desired,
        scope: ticket.scope,
        profile: Rc::new(LoadedProfile {
            identity: ticket.identity,
            config_sha256: metadata["config_sha256"]
                .as_str()
                .context("missing validated config hash")?
                .into(),
            engine,
        }),
    })
}
pub struct Reader {
    scope: String,
    desired: u64,
    target: Option<Identity>,
    active: Option<Rc<LoadedProfile>>,
}
impl Reader {
    pub fn new(scope: &str) -> Result<Self> {
        valid_id(scope)?;
        Ok(Self {
            scope: scope.into(),
            desired: 0,
            target: None,
            active: None,
        })
    }
    pub fn observe(&mut self, snapshot: Snapshot) -> Result<Option<Ticket>> {
        ensure!(snapshot.scope == self.scope, "scope mismatch");
        ensure!(
            snapshot.desired_revision <= 128 && snapshot.desired_revision >= self.desired,
            "desired generation regression"
        );
        let bytes = snapshot
            .active
            .as_ref()
            .map(|p| p.check(&self.scope, snapshot.desired_revision))
            .transpose()?;
        let identity = snapshot.active.as_ref().map(Published::identity);
        if let Some(old) = &self.target {
            let new = identity
                .as_ref()
                .context("active generation regression to empty")?;
            ensure!(
                new.generation >= old.generation,
                "active generation regression"
            );
            ensure!(
                new.generation != old.generation || new == old,
                "same generation has different identity"
            );
        }
        self.desired = snapshot.desired_revision;
        self.target = identity.clone();
        if self.active.as_ref().map(|p| &p.identity) == identity.as_ref() {
            return Ok(None);
        }
        Ok(identity.map(|identity| Ticket {
            scope: self.scope.clone(),
            desired: self.desired,
            identity,
            bytes: bytes.expect("checked active bundle"),
        }))
    }
    pub fn install(&mut self, candidate: Candidate) -> Result<bool> {
        ensure!(
            candidate.scope == self.scope
                && candidate.desired == self.desired
                && self.target.as_ref() == Some(&candidate.profile.identity),
            "superseded load"
        );
        if self
            .active
            .as_ref()
            .is_some_and(|p| p.identity == candidate.profile.identity)
        {
            return Ok(false);
        }
        self.active = Some(candidate.profile);
        Ok(true)
    }
    pub fn acquire(&self) -> Result<Rc<LoadedProfile>> {
        self.active.clone().context("no loaded profile")
    }
    pub fn status(&self) -> Value {
        json!({"scope":self.scope,"desired_revision":self.desired,
            "target_generation":self.target.as_ref().map(|p|p.generation),
            "active_generation":self.active.as_ref().map(|p|p.identity.generation)})
    }
}
