use rt::{CausalContext, VersionVec};

use std::sync::atomic::Ordering::{self, *};

#[derive(Debug, Clone)]
pub struct Synchronize {
    causality: VersionVec,
}

impl Synchronize {
    pub fn new() -> Self {
        Synchronize {
            causality: VersionVec::new(),
        }
    }

    pub fn sync_read(&mut self, ctx: &mut CausalContext, order: Ordering) {
        match order {
            Relaxed | Release => {
                // Nothing happens!
            }
            Acquire | AcqRel => {
                self.sync_acq(ctx);
            }
            SeqCst => {
                self.sync_acq(ctx);
                ctx.seq_cst();
            }
            order => unimplemented!("unimplemented ordering {:?}", order),
        }
    }

    pub fn sync_write(&mut self, ctx: &mut CausalContext, order: Ordering) {
        match order {
            Relaxed | Acquire => {
                // Nothing happens!
            }
            Release | AcqRel => {
                self.sync_rel(ctx);
            }
            SeqCst => {
                self.sync_rel(ctx);
                ctx.seq_cst();
            }
            order => unimplemented!("unimplemented ordering {:?}", order),
        }
    }

    /*
    pub fn sync_read_write(&mut self, ctx: &mut CausalContext, order: Ordering) {
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
    }
    */

    fn sync_acq(&mut self, ctx: &mut CausalContext) {
        ctx.join(&self.causality);
    }

    fn sync_rel(&mut self, ctx: &mut CausalContext) {
        self.causality.join(ctx.actor().causality());
    }
}
