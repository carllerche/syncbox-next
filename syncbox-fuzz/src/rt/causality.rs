use rt::{self, VersionVec};

use std::cell::RefCell;
use std::sync::atomic::Ordering::{self, *};

pub struct Causality {
    version: RefCell<VersionVec>,
}

impl Causality {
    pub fn new() -> Self {
        let vv = rt::with_version(|vv, _| vv.clone());

        Causality {
            version: RefCell::new(vv),
        }
    }

    pub fn sync_read(&self, order: Ordering) {
        match order {
            Relaxed => {
                // Nothing happens!
            }
            Acquire | AcqRel => {
                rt::with_version(|vv, _| {
                    self.sync_acq(vv);
                });
            }
            Release => {
                panic!("invalid ordering");
            }
            SeqCst => {
                rt::with_version(|vv, _| {
                    self.sync_acq(vv);
                    unimplemented!();
                });
            }
            order => unimplemented!("unimplemented ordering {:?}", order),
        }
    }

    pub fn sync_write(&self, order: Ordering) {
        match order {
            Relaxed => {
                // Nothing happens!
            }
            Release | AcqRel => {
                rt::with_version(|vv, id| {
                    self.sync_rel(vv, id);
                });
            }
            Acquire => {
                panic!("invalid ordering");
            }

            SeqCst => {
                rt::with_version(|vv, id| {
                    self.sync_rel(vv, id);
                    unimplemented!();
                });
            }
            order => unimplemented!("unimplemented ordering {:?}", order),
        }
    }

    pub fn sync_read_write(&self, order: Ordering) {
        match order {
            Relaxed => {
            }
            Acquire => {
                rt::with_version(|vv, _| {
                    self.sync_acq(vv);
                });
            }
            Release => {
                rt::with_version(|vv, id| {
                    self.sync_rel(vv, id);
                });
            }
            AcqRel => {
                rt::with_version(|vv, id| {
                    self.sync_acq(vv);
                    self.sync_rel(vv, id);
                });
            }
            SeqCst => {
                rt::with_version(|vv, id| {
                    self.sync_acq(vv);
                    self.sync_rel(vv, id);
                    unimplemented!();
                });
            }
            order => unimplemented!("unimplemented ordering {:?}", order),
        }
    }

    fn sync_acq(&self, thread_version: &mut VersionVec) {
        let rx = self.version.borrow();
        thread_version.join(&rx);
    }

    fn sync_rel(&self, thread_version: &mut VersionVec, id: usize) {
        let mut tx = self.version.borrow_mut();
        tx.join(&thread_version);
        thread_version.inc(id);
    }
}
