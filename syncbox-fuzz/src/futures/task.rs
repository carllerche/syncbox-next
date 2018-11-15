use rt::{self, ThreadHandle};

pub fn current() -> Task {
    Task { thread: ThreadHandle::current() }
}

pub struct Task {
    thread: ThreadHandle,
}

impl Task {
    pub fn notify(&self) {
        self.thread.unpark();
    }
}
