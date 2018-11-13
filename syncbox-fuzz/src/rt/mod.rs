mod causality;
mod execution;
mod thread;
mod vv;

pub use self::causality::Causality;
use self::execution::Execution;
pub use self::execution::JoinHandle;
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

pub fn spawn<F>(f: F) -> JoinHandle
where
    F: FnOnce() + 'static,
{
    Execution::spawn(f)
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

pub fn thread_done() {
    Execution::thread_done();
}

/*
/// Start an execution
pub fn start<F, T>(prev: Option<execution::Execution>, f: F) -> (execution::Execution, T)
where
    F: FnOnce() -> T,
{
    /*
    let (execution, thread) = execution::create(prev);
    let ret = enter(thread, f);
    (execution, ret)
    */
    unimplemented!();
}

/// Return a handle to the current thread
pub fn current() -> Thread {
    unimplemented!();
    /*
    CURRENT_THREAD.with(|cell| {
        match *cell.borrow() {
            Some(ref th) => th.clone(),
            None => unimplemented!(),
        }
    })
    */
}

/// Fork an execution thread
pub fn fork() -> Thread {
    /*
    CURRENT_THREAD.with(|cell| {
        match *cell.borrow() {
            Some(ref th) => th.fork(),
            None => unimplemented!(),
        }
    })
    */
    unimplemented!();
}

/// Add an execution branch point.
pub fn branch() {
    /*
    CURRENT_THREAD.with(|cell| {
        match *cell.borrow() {
            Some(ref th) => th.branch(),
            None => unimplemented!(),
        }
    })
    */
    unimplemented!();
}

/// Mark the current thread as blocked.
pub fn blocked() {
    /*
    CURRENT_THREAD.with(|cell| {
        match *cell.borrow() {
            Some(ref th) => th.blocked(),
            None => unimplemented!(),
        }
    })
    */
    unimplemented!();
}

pub fn enter<F, T>(thread: Thread, f: F) -> T
where
    F: FnOnce() -> T,
{
    /*
    thread.enter();

    CURRENT_THREAD.with(|cell| {
        *cell.borrow_mut() = Some(thread);
        let ret = f();
        cell.borrow().as_ref().unwrap().terminate();
        ret
    })
    */
    unimplemented!();
}
*/
