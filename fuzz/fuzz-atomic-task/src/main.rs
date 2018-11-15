#[macro_use]
extern crate cfg_if;
extern crate futures as _futures;
extern crate syncbox_fuzz;

#[path = "../../../src/futures/atomic_task2.rs"]
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
                println!("+ thread run");
                chan.num.fetch_add(1, Relaxed);
                println!(" + INC");
                chan.task.notify();
                println!(" + NOTIFY");
            });
        }

        poll_fn(move || {
            chan.task.register();
            println!("+ rx registered");

            if 1 == chan.num.load(Relaxed) {
                println!(" + rx ready");
                return Ok(Async::Ready(()));
            }

            println!(" + not ready");
            Ok(Async::NotReady)
        })
    });
}
