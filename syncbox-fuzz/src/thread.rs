use rt::{self, oneshot};

use std::thread as std;

pub struct JoinHandle<T> {
    rx: oneshot::Receiver<std::Result<T>>,
}

pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T,
    F: 'static,
    T: 'static,
{
    let (tx, rx) = oneshot::channel();

    rt::spawn(move || {
        let res = Ok(f());

        rt::seq_cst();

        tx.send(res);
    });

    JoinHandle {
        rx,
    }
}

impl<T> JoinHandle<T> {
    pub fn join(self) -> std::Result<T> {
        let ret = self.rx.recv();
        rt::seq_cst();
        ret
    }
}
