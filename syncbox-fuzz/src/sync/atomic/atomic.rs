use rt;

use std::cell::Cell;
use std::sync::atomic::Ordering;

pub struct Atomic<T> {
    val: Cell<T>,
    rt: rt::Causality,
}

impl<T> Atomic<T>
where
    T: Copy + PartialEq,
{
    pub fn new(val: T) -> Atomic<T> {
        Atomic {
            val: Cell::new(val),
            rt: rt::Causality::new(),
        }
    }

    pub fn load(&self, order: Ordering) -> T {
        rt::branch();
        self.rt.sync_read(order);
        self.val.get()
    }

    pub fn store(&self, val: T, order: Ordering) {
        rt::branch();
        self.rt.sync_write(order);
        self.val.set(val);
    }

    pub fn swap(&self, val: T, order: Ordering) -> T {
        rt::branch();
        self.rt.sync_read_write(order);
        self.val.replace(val)
    }

    pub fn compare_and_swap(&self, current: T, new: T, order: Ordering) -> T {
        rt::branch();
        self.rt.sync_read_write(order);

        let actual = self.val.get();

        if actual == current {
            self.val.set(new);
        }

        actual
    }

    pub fn compare_exchange(
        &self,
        current: T,
        new: T,
        success: Ordering,
        failure: Ordering
    ) -> Result<T, T>
    {
        rt::branch();

        let actual = self.val.get();

        if actual == current {
            self.rt.sync_read_write(success);
            self.val.set(new);
            Ok(actual)
        } else {
            self.rt.sync_read_write(failure);
            Err(actual)
        }
    }

    pub fn update<F>(&self, f: F, order: Ordering) -> T
    where
        F: FnOnce(T) -> T,
    {
        rt::branch();
        self.rt.sync_read_write(order);
        let actual = self.val.get();
        self.val.set(f(actual));
        actual
    }
}
