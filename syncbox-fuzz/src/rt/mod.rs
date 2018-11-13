mod causality;
mod execution;
pub mod oneshot;
mod thread;
mod vv;

pub use self::causality::Causality;
use self::execution::Execution;
pub use self::execution::ThreadHandle;
pub use self::vv::VersionVec;

use std::sync::Arc;

pub fn check<F>(f: F)
where
    F: Fn() + Sync + Send + 'static,
{
    let f = Arc::new(f);

    let mut execution = {
        let f = f.clone();
        Execution::new(move || f())
    };

    execution.run();

    let mut i = 0;

    while let Some(next) = execution.next_seed() {
        i += 1;

        if i % 10_000 == 0 {
            println!("iter {}", i);
        }

        let f = f.clone();
        execution = Execution::with_seed(next, move || f());
        execution.run();
    }
}

pub fn spawn<F>(f: F)
where
    F: FnOnce(ThreadHandle) + 'static,
{
    Execution::spawn(f)
}

pub fn acquire(th: ThreadHandle) {
    Execution::acquire(th);
}

/// Returns a handle to the current thread
pub fn current() -> ThreadHandle {
    Execution::current()
}

/// Marks the current thread as blocked
pub fn park() {
    Execution::park()
}

/// Add an execution branch point.
pub fn branch() {
    Execution::branch();
}

/// Critical section, may not branch.
pub fn critical<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    Execution::critical(f)
}

pub fn with_version<F, R>(f: F) -> R
where
    F: FnOnce(&mut VersionVec, usize) -> R
{
    Execution::with_version(f)
}

if_futures! {
    use _futures::Future;

    pub fn wait_future<F>(f: F)
    where
        F: Future<Item = (), Error = ()>
    {
        unimplemented!();
    }
}
