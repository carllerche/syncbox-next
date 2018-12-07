pub mod arena;
mod execution;
mod fn_box;
pub mod oneshot;
mod scheduler;
mod synchronize;
mod vv;

use self::fn_box::FnBox;
pub(crate) use self::synchronize::Synchronize;
pub(crate) use self::execution::{ThreadHandle, ThreadSet};
pub(crate) use self::vv::{Actor, VersionVec};

// TODO: Cleanup?
pub use self::execution::Branch;

pub(crate) use self::execution::Execution;
pub(crate) use self::scheduler::Scheduler;

pub fn spawn<F>(f: F)
where
    F: FnOnce() + 'static,
{
    Scheduler::with_execution(|execution| {
        execution.create_thread();
    });

    Scheduler::spawn(Box::new(move || {
        f();
        thread_done();
    }));

}

/// Marks the current thread as blocked
pub fn park() {
    Scheduler::with_execution(|execution| {
        execution.threads.active_mut().set_blocked();
    });

    Scheduler::switch();
}

/// Add an execution branch point.
pub fn branch() {
    Scheduler::switch();
}

pub fn yield_now() {
    Scheduler::with_execution(|execution| {
        execution.threads.active_mut().set_yield();
    });

    Scheduler::switch();
}

/// Critical section, may not branch.
pub fn critical<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    struct Reset;

    impl Drop for Reset {
        fn drop(&mut self) {
            Scheduler::with_execution(|execution| {
                execution.unset_critical();
            });
        }
    }

    let _reset = Reset;

    Scheduler::with_execution(|execution| {
        execution.set_critical();
    });

    f()
}

pub(crate) fn execution<F, R>(f: F) -> R
where
    F: FnOnce(&mut Execution) -> R
{
    Scheduler::with_execution(f)
}

pub fn seq_cst() {
    branch();
    Scheduler::with_execution(|execution| {
        execution.seq_cst();
    });
}

if_futures! {
    use _futures::Future;
    use std::mem::replace;

    pub fn wait_future<F>(mut f: F)
    where
        F: Future<Item = (), Error = ()>
    {
        loop {
            let res = f.poll().unwrap();

            if res.is_ready() {
                return;
            }

            let notified = Scheduler::with_execution(|execution| {
                replace(
                    &mut execution.threads.active_mut().notified,
                    false)

            });

            if !notified {
                park();
            }
        }
    }
}

pub fn thread_done() {
    Scheduler::with_execution(|execution| {
        execution.threads.active_mut().set_terminated();
    });
}
