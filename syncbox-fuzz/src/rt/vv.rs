use rt::ThreadSet;
use rt::arena::{self, Arena};

use std::cmp;
use std::ops;

#[derive(Debug, PartialOrd, Eq, PartialEq)]
pub struct VersionVec {
    versions: arena::Slice<usize>,
}

#[derive(Debug)]
pub struct Actor<'a> {
    vv: &'a mut VersionVec,
    id: usize,
}

impl VersionVec {
    pub fn new(max_threads: usize, arena: &mut Arena) -> VersionVec {
        VersionVec {
            versions: arena.slice(max_threads),
        }
    }

    /// Returns a new `VersionVec` that represents the very first event in the execution.
    pub fn root(max_threads: usize, arena: &mut Arena) -> VersionVec {
        let mut vv = VersionVec::new(max_threads, arena);
        vv[0] += 1;
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
        for (i, &version) in other.versions.iter().enumerate() {
            self.versions[i] = cmp::max(self.versions[i], version);
        }
    }

    pub fn clone_with(&self, arena: &mut arena::Arena) -> Self {
        VersionVec {
            versions: self.versions.clone_with(arena),
        }
    }
}

impl ops::Index<usize> for VersionVec {
    type Output = usize;

    fn index(&self, index: usize) -> &usize {
        self.versions.index(index)
    }
}

impl ops::IndexMut<usize> for VersionVec {
    fn index_mut(&mut self, index: usize) -> &mut usize {
        self.versions.index_mut(index)
    }
}

impl<'a> Actor<'a> {
    pub(super) fn new(ctx: &'a mut ThreadSet) -> Actor<'a> {
        Actor {
            vv: &mut ctx.threads[ctx.active].causality,
            id: ctx.active,
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
        self.vv.versions[self.id] += 1;
    }

    pub fn join(&mut self, other: &VersionVec) {
        self.vv.join(other);
    }
}
