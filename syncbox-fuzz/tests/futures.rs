#![cfg(feature = "futures")]

extern crate futures;
extern crate syncbox_fuzz;

use syncbox_fuzz::fuzz_future;
use syncbox_fuzz::futures::task;
use syncbox_fuzz::sync::atomic::AtomicUsize;
use syncbox_fuzz::thread;

use futures::Async;
use futures::future::poll_fn;

use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

#[test]
fn fuzz_valid() {
    fuzz_future(|| {
        let num = Arc::new(AtomicUsize::new(0));
        let task = task::current();

        thread::spawn({
            let num = num.clone();

            move || {
                num.store(1, Relaxed);
                task.notify();
            }
        });

        poll_fn(move || {
            if 1 == num.load(Relaxed) {
                Ok(Async::Ready(()))
            } else {
                Ok(Async::NotReady)
            }
        })
    });
}
