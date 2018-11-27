mod execution;
pub mod oneshot;
mod scheduler;
mod synchronize;
mod thread;
mod vv;

pub use self::synchronize::Synchronize;
pub use self::execution::{ThreadHandle};
pub use self::vv::{Actor, CausalContext, VersionVec};

// TODO: Cleanup?
pub use self::execution::Branch;

use self::execution::{Execution, Seed};
use self::scheduler::Scheduler;
use self::thread::Thread;

use std::sync::Arc;

pub fn check<F>(f: F)
where
    F: Fn() + Sync + Send + 'static,
{
    let f = Arc::new(f);

    let mut execution = {
        let f = f.clone();
        Scheduler::new(move || f())
    };

    execution.run();

    let mut i = 0;

    while let Some(next) = execution.next_seed() {
        i += 1;

        if i % 10_000 == 0 {
            println!("+++++++++ iter {}", i);
        }

        let f = f.clone();
        execution = Scheduler::with_seed(next, move || f());
        execution.run();
    }
}

pub fn spawn<F>(f: F)
where
    F: FnOnce() + 'static,
{
    Scheduler::spawn(f)
}

/// Marks the current thread as blocked
pub fn park() {
    Scheduler::park()
}

/// Add an execution branch point.
pub fn branch() {
    Scheduler::branch(false);
}

pub fn yield_now() {
    Scheduler::branch(true);
}

/// Critical section, may not branch.
pub fn critical<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    struct Reset;

    impl Drop for Reset {
        fn drop(&mut self) {
            Execution::with(|execution| {
                execution.unset_critical();
            });
        }
    }

    let _reset = Reset;

    Execution::with(|execution| {
        execution.set_critical();
    });

    f()
}

pub fn causal_context<F, R>(f: F) -> R
where
    F: FnOnce(&mut CausalContext) -> R
{
    Execution::with(|execution| {
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
