use rt::{self, oneshot};

use std::thread as std;

pub struct JoinHandle<T> {
    rx: oneshot::Receiver<(std::Result<T>, rt::ThreadHandle)>,
}

pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T,
    F: 'static,
    T: 'static,
{
    let (tx, rx) = oneshot::channel();

    rt::spawn(move |th| {
        tx.send((Ok(f()), th));
    });

    JoinHandle {
        rx,
    }
}

impl<T> JoinHandle<T> {
    pub fn join(self) -> std::Result<T> {
        let (res, th) = self.rx.recv();
        rt::acquire(th);
        res
    }
}
