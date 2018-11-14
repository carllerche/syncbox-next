use rt::Execution;

use std::cmp;

#[derive(Debug, Clone, PartialOrd, Eq, PartialEq)]
pub struct VersionVec {
    versions: Vec<usize>,
}

#[derive(Debug)]
pub struct Actor<'a> {
    vv: &'a mut VersionVec,
    id: usize,
}

/// Current causal context
#[derive(Debug)]
pub struct CausalContext<'a> {
    actor: Actor<'a>,
    seq_cst_causality: &'a mut VersionVec,
}

impl VersionVec {
    pub fn new() -> VersionVec {
        VersionVec {
            versions: vec![],
        }
    }

    /// Returns a new `VersionVec` that represents the very first event in the execution.
    pub fn root() -> VersionVec {
        let mut vv = VersionVec::new();
        Actor::new(&mut vv, 0).inc();
        vv
    }

    pub fn join(&mut self, other: &VersionVec) {
        if self.versions.len() < other.versions.len() {
            self.versions.resize(other.versions.len(), 0);
        }

        for (i, &version) in other.versions.iter().enumerate() {
            self.versions[i] = cmp::max(self.versions[i], version);
        }
    }
}

impl<'a> Actor<'a> {
    pub(super) fn new(vv: &'a mut VersionVec, id: usize) -> Actor<'a> {
        Actor {
            vv,
            id,
        }
    }

    pub fn inc(&mut self) {
        if self.vv.versions.len() <= self.id {
            self.vv.versions.resize(self.id + 1, 0);
        }

        self.vv.versions[self.id] += 1;
    }
}

impl<'a> CausalContext<'a> {
    pub fn new(execution: &'a mut Execution) -> CausalContext<'a> {
        CausalContext {
            actor: Actor {
                vv: &mut execution.threads[execution.active_thread].causality,
                id: execution.active_thread,
            },
            seq_cst_causality: &mut execution.seq_cst_causality,
        }
    }

    pub fn join(&mut self, other: &VersionVec) {
        self.actor.vv.join(other);
    }

    pub fn version(&self) -> &VersionVec {
        &self.actor.vv
    }

    pub fn actor(&mut self) -> &mut Actor<'a> {
        &mut self.actor
    }
}
