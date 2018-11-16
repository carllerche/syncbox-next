use rt::{Branch, Execution};

use std::cmp;
use std::collections::VecDeque;
use std::ops;

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

    // TODO: Cleanup
    pub seed: &'a mut VecDeque<Branch>,
    pub branches: &'a mut Vec<Branch>,
}

static NULL: usize = 0;
const INIT: usize = 1;

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

    // TODO: Iterate over `(ThreadId, Version)`
    pub fn versions<'a>(&'a self) -> impl Iterator<Item = (usize, usize)> + 'a {
        self.versions.iter()
            .map(|v| *v)
            .enumerate()
    }

    pub fn len(&self) -> usize {
        self.versions.len()
    }

    pub fn join(&mut self, other: &VersionVec) {
        if self.versions.len() < other.versions.len() {
            self.versions.resize(other.versions.len(), INIT);
        }

        for (i, &version) in other.versions.iter().enumerate() {
            self.versions[i] = cmp::max(self.versions[i], version);
        }
    }
}

impl ops::Index<usize> for VersionVec {
    type Output = usize;

    fn index(&self, index: usize) -> &usize {
        self.versions.get(index)
            .unwrap_or(&NULL)
    }
}

impl ops::IndexMut<usize> for VersionVec {
    fn index_mut(&mut self, index: usize) -> &mut usize {
        if self.versions.len() < index + 1 {
            self.versions.resize(index + 1, INIT);
        }

        &mut self.versions[index]
    }
}

impl<'a> Actor<'a> {
    pub(super) fn new(vv: &'a mut VersionVec, id: usize) -> Actor<'a> {
        Actor {
            vv,
            id,
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn happens_before(&self) -> &VersionVec {
        &self.vv
    }

    // TODO: rename `version`
    pub fn self_version(&self) -> usize {
        self.vv[self.id]
    }

    pub fn inc(&mut self) {
        if self.vv.versions.len() <= self.id {
            self.vv.versions.resize(self.id + 1, INIT);
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
            seed: &mut execution.seed,
            branches: &mut execution.branches,
        }
    }

    pub fn join(&mut self, other: &VersionVec) {
        self.actor.vv.join(other);
    }

    pub fn actor(&mut self) -> &mut Actor<'a> {
        &mut self.actor
    }

    /// Insert a point of sequential consistency
    pub fn seq_cst(&mut self) {
        self.actor.vv.join(self.seq_cst_causality);
        self.seq_cst_causality.join(self.actor.vv);
    }
}
