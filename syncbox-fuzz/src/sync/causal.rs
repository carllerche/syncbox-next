use rt::{self, VersionVec};

use std::cell::{RefCell, UnsafeCell};

pub struct CausalCell<T> {
    data: UnsafeCell<T>,
    version: RefCell<VersionVec>,
}

impl<T> CausalCell<T> {
    pub fn new(data: T) -> CausalCell<T> {
        let v = rt::causal_context(|ctx| {
            ctx.actor().causality().clone()
        });

        CausalCell {
            data: UnsafeCell::new(data),
            version: RefCell::new(v),
        }
    }

    pub unsafe fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        rt::causal_context(|ctx| {
            let v = self.version.borrow();

            assert!(
                *v <= *ctx.actor().causality(),
                "cell={:?}; thread={:?}",
                *v, *ctx.actor().causality());
        });

        rt::critical(|| {
            f(&*self.data.get())
        })
    }

    pub unsafe fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        rt::causal_context(|ctx| {
            let mut v = self.version.borrow_mut();

            assert!(
                *v <= *ctx.actor().causality(),
                "cell={:?}; thread={:?}",
                *v, *ctx.actor().causality());

            v.join(ctx.actor().causality());
        });

        rt::critical(|| {
            f(&mut *self.data.get())
        })
    }
}
