use rt::{self, ThreadHandle};

use std::cell::{UnsafeCell, Cell, RefCell};
use std::collections::VecDeque;
use std::sync::LockResult;

pub struct Mutex<T> {
    #[allow(unused)]
    data: UnsafeCell<T>,
    locked: Cell<Option<ThreadHandle>>,
    waiters: RefCell<VecDeque<ThreadHandle>>,
}

pub struct MutexGuard<'a, T: 'a> {
    lock: &'a Mutex<T>,
}

impl<T> Mutex<T> {
    pub fn new(data: T) -> Mutex<T> {
        Mutex {
            data: UnsafeCell::new(data),
            locked: Cell::new(None),
            waiters: RefCell::new(VecDeque::new()),
        }
    }
}

impl<T> Mutex<T> {
    pub fn lock(&self) -> LockResult<MutexGuard<T>> {
        let guard = MutexGuard {
            lock: self,
        };

        guard.acquire(ThreadHandle::current());
        Ok(guard)
    }

    pub(super) fn owner(&self) -> Option<ThreadHandle> {
        self.locked.get()
    }
}

impl<'a, T: 'a> MutexGuard<'a, T> {
    pub(super) fn mutex(&self) -> &Mutex<T> {
        self.lock
    }

    pub(super) fn acquire(&self, th: ThreadHandle) {
        rt::branch();

        assert_ne!(self.lock.locked.get(), Some(th), "cannot re-enter mutex");

        if self.lock.locked.get().is_none() {
            self.lock.locked.set(Some(th));
        } else {
            self.lock.waiters.borrow_mut()
                .push_back(th);
        }

        while self.lock.locked.get() != Some(th) {
            rt::park();
        }
    }

    pub(super) fn release(&self) {
        let next = self.lock.waiters.borrow_mut().pop_front();
        self.lock.locked.set(next);

        if let Some(th) = next {
            th.unpark();
        }

        rt::branch();
    }
}

impl<'a, T: 'a> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        self.release();
    }
}
