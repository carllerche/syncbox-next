pub(crate) mod arena;
mod execution;
mod fn_box;
pub(crate) mod object;
pub(crate) mod oneshot;
mod path;
mod scheduler;
mod synchronize;
pub(crate) mod thread;
mod vv;

use self::fn_box::FnBox;
pub(crate) use self::synchronize::Synchronize;
pub(crate) use self::path::Path;
pub(crate) use self::vv::VersionVec;

pub(crate) use self::execution::Execution;
pub(crate) use self::scheduler::Scheduler;

pub fn spawn<F>(f: F)
where
    F: FnOnce() + 'static,
{
    execution(|execution| {
        execution.new_thread();
    });

    Scheduler::spawn(Box::new(move || {
        f();
        thread_done();
    }));

}

/// Marks the current thread as blocked
pub fn park() {
    execution(|execution| {
        execution.threads.active_mut().set_blocked();
        execution.threads.active_mut().operation = None;
        execution.schedule()
    });

    Scheduler::switch();
}

/// Add an execution branch point.
fn branch<F, R>(f: F) -> R
where
    F: FnOnce(&mut Execution) -> R,
{
    let (ret, switch) = execution(|execution| {
        let ret = f(execution);
        (ret, execution.schedule())
    });

    if switch {
        Scheduler::switch();
    }

    ret
}

pub fn yield_now() {
    let switch = execution(|execution| {
        execution.threads.active_mut().set_yield();
        execution.threads.active_mut().operation = None;
        execution.schedule()
    });

    if switch {
        Scheduler::switch();
    }
}

/// Critical section, may not branch.
pub fn critical<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    struct Reset;

    impl Drop for Reset {
        fn drop(&mut self) {
            execution(|execution| {
                execution.unset_critical();
            });
        }
    }

    let _reset = Reset;

    execution(|execution| {
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

            let notified = execution(|execution| {
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
    execution(|execution| {
        execution.threads.active_mut().set_terminated();
        execution.threads.active_mut().operation = None;
        execution.schedule()
    });
}
