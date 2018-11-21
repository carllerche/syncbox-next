#[macro_use]
extern crate cfg_if;
extern crate syncbox_fuzz;

#[path = "../../../src/parker.rs"]
mod parker;

use parker::*;

use syncbox_fuzz::{
    fuzz,
    sync::atomic::AtomicUsize,
    thread,
};

use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

struct Chan {
    num: AtomicUsize,
    parker: Parker,
}

const NUM: usize = 1;

fn main() {
    fuzz(|| {
        let chan = Arc::new(Chan {
            num: AtomicUsize::new(0),
            parker: Parker::new(),
        });

        for _ in 0..NUM {
            let chan = chan.clone();

            thread::spawn(move || {
                chan.num.fetch_add(1, Relaxed);
                chan.parker.unpark();
            });
        }

        loop {
            if NUM == chan.num.load(Relaxed) {
                break;
            }

            chan.parker.park();
        }
    });
}
