mod execution;
mod fn_box;
pub mod oneshot;
mod scheduler;
mod synchronize;
mod thread;
mod vv;

use self::fn_box::FnBox;
pub use self::synchronize::Synchronize;
pub use self::execution::{ThreadHandle};
pub use self::vv::{Actor, CausalContext, VersionVec};

// TODO: Cleanup?
pub use self::execution::Branch;

use self::execution::Execution;
use self::scheduler::Scheduler;
use self::thread::Thread;

const DEFAULT_MAX_THREADS: usize = 4;

pub fn check<F>(f: F)
where
    F: Fn() + Sync + Send + 'static,
{
    let mut execution = Execution::new();
    let mut scheduler = Scheduler::new(DEFAULT_MAX_THREADS, move || {
        f();
        thread_done();
    });

    scheduler.run(&mut execution);

    let mut i = 0;

    while execution.step() {
        i += 1;

        if i % 10_000 == 0 {
            println!("+++++++++ iter {}", i);
        }

        scheduler.run(&mut execution);
    }
}

pub fn spawn<F>(f: F)
where
    F: FnOnce() + 'static,
{
    Scheduler::with_execution(|execution| {
        execution.create_thread();
        execution.queued_spawn.push_back(Box::new(move || {
            f();
            thread_done();
        }));
    })
}

/// Marks the current thread as blocked
pub fn park() {
    Scheduler::with_execution(|execution| {
        execution.active_thread_mut().set_blocked();
    });

    Scheduler::switch();
}

/// Add an execution branch point.
pub fn branch() {
    Scheduler::switch();
}

pub fn yield_now() {
    Scheduler::with_execution(|execution| {
        execution.active_thread_mut().set_yield();
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

pub fn causal_context<F, R>(f: F) -> R
where
    F: FnOnce(&mut CausalContext) -> R
{
    Scheduler::with_execution(|execution| {
        f(&mut CausalContext::new(execution))
    })
}

pub fn seq_cst() {
    branch();
    causal_context(|ctx| ctx.seq_cst());
}

if_futures! {
    mod futures;

    use _futures::Future;

    pub fn wait_future<F>(f: F)
    where
        F: Future<Item = (), Error = ()>
    {
        use _futures::executor::spawn;
        use std::sync::Arc;

        let notify = Arc::new(futures::Notify::new());
        let mut f = spawn(f);

        loop {
            let res = f.poll_future_notify(&notify, 0).unwrap();

            if res.is_ready() {
                return;
            }

            if !notify.consume_notify() {
                park();
            }
        }
    }
}

fn thread_done() {
    Scheduler::with_execution(|execution| {
        execution.active_thread_mut().set_terminated();
    });
}
