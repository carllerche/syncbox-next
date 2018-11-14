use rt::{self, CausalContext, VersionVec};

use std::cell::RefCell;
use std::sync::atomic::Ordering::{self, *};

pub struct Synchronize {
    version: RefCell<VersionVec>,
}

impl Synchronize {
    pub fn new() -> Self {
        let vv = rt::causal_context(|ctx| {
            ctx.version().clone()
        });

        Synchronize {
            version: RefCell::new(vv),
        }
    }

    pub fn sync_read(&self, order: Ordering) {
        match order {
            Relaxed => {
                // Nothing happens!
            }
            Acquire | AcqRel => {
                rt::causal_context(|ctx| {
                    self.sync_acq(ctx);
                });
            }
            Release => {
                panic!("invalid ordering");
            }
            SeqCst => {
                rt::causal_context(|ctx| {
                    self.sync_acq(ctx);
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
                rt::causal_context(|ctx| {
                    self.sync_rel(ctx);
                });
            }
            Acquire => {
                panic!("invalid ordering");
            }

            SeqCst => {
                rt::causal_context(|ctx| {
                    self.sync_rel(ctx);
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
                rt::causal_context(|ctx| {
                    self.sync_acq(ctx);
                });
            }
            Release => {
                rt::causal_context(|ctx| {
                    self.sync_rel(ctx);
                });
            }
            AcqRel => {
                rt::causal_context(|ctx| {
                    self.sync_acq(ctx);
                    self.sync_rel(ctx);
                });
            }
            SeqCst => {
                rt::causal_context(|ctx| {
                    self.sync_acq(ctx);
                    self.sync_rel(ctx);
                    unimplemented!();
                });
            }
            order => unimplemented!("unimplemented ordering {:?}", order),
        }
    }

    fn sync_acq(&self, ctx: &mut CausalContext) {
        let rx = self.version.borrow();
        ctx.join(&rx);
    }

    fn sync_rel(&self, ctx: &mut CausalContext) {
        let mut tx = self.version.borrow_mut();
        tx.join(ctx.version());
        ctx.actor().inc();
    }
}
