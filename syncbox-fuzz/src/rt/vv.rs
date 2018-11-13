use std::cmp::{self, PartialOrd};

#[derive(Debug, Clone, PartialOrd, Eq, PartialEq)]
pub struct VersionVec {
    versions: Vec<usize>,
}

impl VersionVec {
    pub fn new() -> VersionVec {
        VersionVec {
            versions: vec![],
        }
    }

    pub fn join(&mut self, other: &VersionVec) {
        if self.versions.len() < other.versions.len() {
            self.versions.resize(other.versions.len(), 0);
        }

        for (i, &version) in other.versions.iter().enumerate() {
            self.versions[i] = cmp::max(self.versions[i], version);
        }
    }

    pub fn inc(&mut self, id: usize) {
        if self.versions.len() <= id {
            self.versions.resize(id + 1, 0);
        }

        self.versions[id] += 1;
    }
}
