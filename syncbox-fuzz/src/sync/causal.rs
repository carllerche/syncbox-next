use rt::{self, VersionVec};

use std::cell::{RefCell, UnsafeCell};

pub struct CausalCell<T> {
    data: UnsafeCell<T>,
    version: RefCell<VersionVec>,
}

impl<T> CausalCell<T> {
    pub fn new(data: T) -> CausalCell<T> {
        let v = rt::execution(|execution| {
            execution.threads.actor()
                .happens_before()
                .clone_with(&mut execution.arena)
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
        rt::execution(|execution| {
            let v = self.version.borrow();

            assert!(
                *v <= *execution.threads.actor().happens_before(),
                "cell={:?}; thread={:?}",
                *v, *execution.threads.actor().happens_before());
        });

        rt::critical(|| {
            f(&*self.data.get())
        })
    }

    pub unsafe fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        rt::execution(|execution| {
            let mut v = self.version.borrow_mut();

            assert!(
                *v <= *execution.threads.actor().happens_before(),
                "cell={:?}; thread={:?}",
                *v, *execution.threads.actor().happens_before());

            v.join(execution.threads.actor().happens_before());
        });

        rt::critical(|| {
            f(&mut *self.data.get())
        })
    }
}
