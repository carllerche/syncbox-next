use rt::{self, ThreadHandle};
use _futures::executor;

pub struct Notify {
    thread: ThreadHandle,
}

impl Notify {
    pub fn new() -> Notify {
        Notify {
            thread: ThreadHandle::current(),
        }
    }
}

impl executor::Notify for Notify {
    fn notify(&self, _id: usize) {
        self.thread.unpark();
    }
}
