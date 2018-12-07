use rt::{Execution, VersionVec};
use rt::arena::Arena;

use std::sync::atomic::Ordering::{self, *};

#[derive(Debug)]
pub(crate) struct Synchronize {
    happens_before: VersionVec,
}

impl Synchronize {
    pub fn new(max_threads: usize, arena: &mut Arena) -> Self {
        let happens_before =
            VersionVec::new(max_threads, arena);

        Synchronize {
            happens_before,
        }
    }

    pub fn sync_read(&mut self, execution: &mut Execution, order: Ordering) {
        match order {
            Relaxed | Release => {
                // Nothing happens!
            }
            Acquire | AcqRel => {
                self.sync_acq(execution);
            }
            SeqCst => {
                self.sync_acq(execution);
                execution.seq_cst();
            }
            order => unimplemented!("unimplemented ordering {:?}", order),
        }
    }

    pub fn sync_write(&mut self, execution: &mut Execution, order: Ordering) {
        match order {
            Relaxed | Acquire => {
                // Nothing happens!
            }
            Release | AcqRel => {
                self.sync_rel(execution);
            }
            SeqCst => {
                self.sync_rel(execution);
                execution.seq_cst();
            }
            order => unimplemented!("unimplemented ordering {:?}", order),
        }
    }

    pub fn clone_with(&self, arena: &mut Arena) -> Synchronize {
        Synchronize { happens_before: self.happens_before.clone_with(arena) }
    }

    fn sync_acq(&mut self, execution: &mut Execution) {
        execution.threads.actor().join(&self.happens_before);
    }

    fn sync_rel(&mut self, execution: &mut Execution) {
        self.happens_before.join(execution.threads.actor().happens_before());
    }
}
