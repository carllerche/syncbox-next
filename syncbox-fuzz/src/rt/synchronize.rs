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
        rt::causal_context(|ctx| {
            match order {
                Relaxed => {
                    // Nothing happens!
                }
                Acquire | AcqRel => {
                    self.sync_acq(ctx);
                }
                Release => {
                    panic!("invalid ordering");
                }
                SeqCst => {
                    self.sync_acq(ctx);
                    ctx.seq_cst();
                }
                order => unimplemented!("unimplemented ordering {:?}", order),
            }

            ctx.actor().inc();
        });
    }

    pub fn sync_write(&self, order: Ordering) {
        rt::causal_context(|ctx| {
            match order {
                Relaxed => {
                    // Nothing happens!
                }
                Release | AcqRel => {
                    self.sync_rel(ctx);
                }
                Acquire => {
                    panic!("invalid ordering");
                }

                SeqCst => {
                    self.sync_rel(ctx);
                    ctx.seq_cst();
                }
                order => unimplemented!("unimplemented ordering {:?}", order),
            }

            ctx.actor().inc();
        });
    }

    pub fn sync_read_write(&self, order: Ordering) {
        rt::causal_context(|ctx| {
            match order {
                Relaxed => {
                }
                Acquire => {
                    self.sync_acq(ctx);
                }
                Release => {
                    self.sync_rel(ctx);
                }
                AcqRel => {
                    self.sync_acq(ctx);
                    self.sync_rel(ctx);
                }
                SeqCst => {
                    self.sync_acq(ctx);
                    self.sync_rel(ctx);
                    ctx.seq_cst();
                }
                order => unimplemented!("unimplemented ordering {:?}", order),
            }

            ctx.actor().inc();
        });
    }

    fn sync_acq(&self, ctx: &mut CausalContext) {
        let version = self.version.borrow();
        ctx.join(&version);
    }

    fn sync_rel(&self, ctx: &mut CausalContext) {
        let mut version = self.version.borrow_mut();
        version.join(ctx.version());
    }
}
