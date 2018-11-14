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

    pub fn sync(&self, order: Ordering) {
        match order {
            Acquire => {
                rt::with_version(|vv, _| {
                    let rx = self.version.borrow();
                    vv.join(&rx);
                });
            }
            Release => {
                rt::with_version(|vv, id| {
                    let mut tx = self.version.borrow_mut();
                    tx.join(&vv);
                    vv.inc(id);
                });
            }
            Relaxed => {
                // Nothing happens!
            }
            order => unimplemented!("unimplemented ordering {:?}", order),
        }
    }
}
