extern crate syncbox_fuzz;

use syncbox_fuzz::sync::{Condvar, Mutex};
use syncbox_fuzz::sync::atomic::AtomicUsize;
use syncbox_fuzz::thread;

use std::sync::Arc;
use std::sync::atomic::Ordering::{Acquire, Release, Relaxed, SeqCst};

#[test]
fn fuzz_condvar() {
    struct Inc {
        num: AtomicUsize,
        mutex: Mutex<()>,
        condvar: Condvar,
    }

    impl Inc {
        fn new() -> Inc {
            Inc {
                num: AtomicUsize::new(0),
                mutex: Mutex::new(()),
                condvar: Condvar::new(),
            }
        }

        fn inc(&self) {
            self.num.store(1, SeqCst);
            drop(self.mutex.lock().unwrap());
            self.condvar.notify_one();
        }
    }

    syncbox_fuzz::fuzz(|| {
        let inc = Arc::new(Inc::new());

        for _ in 0..1 {
            let inc = inc.clone();
            thread::spawn(move || inc.inc());
        }

        let mut guard = inc.mutex.lock().unwrap();

        let mut i = 0;
        loop {
            i += 1;
            let val = inc.num.load(SeqCst);
            if 1 == val {
                break;
            }

            guard = inc.condvar.wait(guard).unwrap();
        }
    });
}
