use rt::thread;

use std::cmp;
use std::ops;

#[derive(Debug, Clone, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct VersionVec {
    versions: Box<[usize]>,
}

impl VersionVec {
    pub fn new(max_threads: usize) -> VersionVec {
        VersionVec {
            versions: vec![0; max_threads].into_boxed_slice(),
        }
    }

    pub fn versions<'a>(&'a self) -> impl Iterator<Item = (thread::Id, usize)> + 'a {
        self.versions.iter()
            .enumerate()
            .map(|(thread_id, &version)| {
                (thread::Id::from_usize(thread_id), version)
            })
    }

    pub fn len(&self) -> usize {
        self.versions.len()
    }

    pub fn inc(&mut self, id: thread::Id) {
        self.versions[id.as_usize()] += 1;
    }

    pub fn join(&mut self, other: &VersionVec) {
        for (i, &version) in other.versions.iter().enumerate() {
            self.versions[i] = cmp::max(self.versions[i], version);
        }
    }
}

impl ops::Index<thread::Id> for VersionVec {
    type Output = usize;

    fn index(&self, index: thread::Id) -> &usize {
        self.versions.index(index.as_usize())
    }
}

impl ops::IndexMut<thread::Id> for VersionVec {
    fn index_mut(&mut self, index: thread::Id) -> &mut usize {
        self.versions.index_mut(index.as_usize())
    }
}
