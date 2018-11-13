use rt::{self, VersionVec};

use std::cell::{RefCell, UnsafeCell};

pub struct CausalCell<T> {
    data: UnsafeCell<T>,
    version: RefCell<VersionVec>,
}

impl<T> CausalCell<T> {
    pub fn new(data: T) -> CausalCell<T> {
        let v = rt::with_version(|v, _| v.clone());

        CausalCell {
            data: UnsafeCell::new(data),
            version: RefCell::new(v),
        }
    }

    pub unsafe fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        rt::with_version(|th_version, _| {
            let v = self.version.borrow();
            assert!(*v <= *th_version, "cell={:?}; thread={:?}", *v, *th_version);
        });

        rt::critical(|| {
            f(&*self.data.get())
        })
    }

    pub unsafe fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        rt::with_version(|th_version, _| {
            let mut v = self.version.borrow_mut();
            assert!(*v <= *th_version, "cell={:?}; thread={:?}", *v, *th_version);
            v.join(th_version);
        });

        rt::critical(|| {
            f(&mut *self.data.get())
        })
    }
}
