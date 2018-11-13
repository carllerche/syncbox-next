use rt;

use std::sync::atomic as std;
use std::sync::atomic::Ordering;

pub struct AtomicUsize {
    std: std::AtomicUsize,
    rt: rt::Causality,
}

impl AtomicUsize {
    pub fn new(v: usize) -> AtomicUsize {
        AtomicUsize {
            std: std::AtomicUsize::new(v),
            rt: rt::Causality::new(),
        }
    }

    pub fn load(&self, order: Ordering) -> usize {
        rt::branch();
        self.rt.sync(order);
        self.std.load(order)
    }

    pub fn store(&self, val: usize, order: Ordering) {
        rt::branch();
        self.rt.sync(order);
        self.std.store(val, order)
    }

    pub fn swap(&self, val: usize, order: Ordering) -> usize {
        rt::branch();
        self.rt.sync(order);
        self.std.swap(val, order)
    }

    pub fn compare_and_swap(&self, current: usize, new: usize, order: Ordering) -> usize {
        rt::branch();
        self.rt.sync(order);
        self.std.compare_and_swap(current, new, order)
    }
}
