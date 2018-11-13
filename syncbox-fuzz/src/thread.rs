use rt;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread as std;

pub struct JoinHandle<T> {
    rx: Rc<RefCell<Option<std::Result<T>>>>,
    rt: rt::JoinHandle,
}

pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T,
    F: 'static,
    T: 'static,
{
    let rx = Rc::new(RefCell::new(None));
    let tx = rx.clone();

    let rt = rt::spawn(move || {
        *tx.borrow_mut() = Some(Ok(f()));

        rt::thread_done();
    });

    JoinHandle {
        rx,
        rt,
    }
}

impl<T> JoinHandle<T> {
    pub fn join(self) -> std::Result<T> {
        self.rt.wait();
        self.rx.borrow_mut()
            .take().unwrap()
    }
}
