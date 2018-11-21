use super::{MutexGuard, LockResult};
use rt::{self, ThreadHandle};

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Duration;

pub struct Condvar {
    waiters: RefCell<VecDeque<ThreadHandle>>,
}

pub struct WaitTimeoutResult(bool);

impl Condvar {
    pub fn new() -> Condvar {
        Condvar {
            waiters: RefCell::new(VecDeque::new()),
        }
    }

    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>)
        -> LockResult<MutexGuard<'a, T>>
    {
        rt::branch();

        let th = ThreadHandle::current();

        assert!(self.is_notified(&th));

        self.waiters.borrow_mut()
            .push_back(th);

        assert_eq!(guard.mutex().owner(), Some(th));

        guard.release();

        if !self.is_notified(&th) {
            rt::yield_now();
            self.waiters.borrow_mut().retain(|waiter| waiter != &th);
        }

        guard.acquire(th);

        Ok(guard)
    }

    pub fn wait_timeout<'a, T>(&self, guard: MutexGuard<'a, T>, dur: Duration)
        -> LockResult<(MutexGuard<'a, T>, WaitTimeoutResult)>
    {
        unimplemented!();
    }

    pub fn notify_one(&self) {
        rt::branch();

        let th = self.waiters.borrow_mut()
            .pop_front();

        if let Some(th) = th {
            th.unpark();
        }
    }

    fn is_notified(&self, th: &ThreadHandle) -> bool {
        !self.waiters.borrow()
            .iter()
            .any(|waiter| waiter == th)
    }
}
