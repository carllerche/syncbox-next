#[macro_use]
extern crate cfg_if;
extern crate futures as _futures;
extern crate syncbox_fuzz;

#[path = "../../../src/futures/atomic_task.rs"]
mod atomic_task;

use atomic_task::AtomicTask;

use syncbox_fuzz::{
    fuzz_future,
    sync::atomic::AtomicUsize,
    thread,
};

use _futures::{
    Async,
    future::poll_fn,
};
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

struct Chan {
    num: AtomicUsize,
    task: AtomicTask,
}

fn main() {
    fuzz_future(|| {
        let chan = Arc::new(Chan {
            num: AtomicUsize::new(0),
            task: AtomicTask::new(),
        });

        for _ in 0..1 {
            let chan = chan.clone();

            thread::spawn(move || {
                chan.num.fetch_add(1, Relaxed);
                chan.task.notify();
            });
        }

        poll_fn(move || {
            chan.task.register();

            if 1 == chan.num.load(Relaxed) {
                return Ok(Async::Ready(()));
            }

            Ok(Async::NotReady)
        })
    });
}
