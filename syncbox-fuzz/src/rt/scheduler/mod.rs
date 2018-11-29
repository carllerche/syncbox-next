mod fringe;

use rt::Execution;
use std::cell::Cell;

#[derive(Debug)]
pub struct Scheduler {
    kind: Kind<fringe::Scheduler>,
}

#[derive(Copy, Clone, Debug)]
enum Kind<T = ()> {
    Fringe(T),
}

use self::Kind::*;

thread_local!(static KIND: Cell<Kind> = Cell::new(Fringe(())));

impl Scheduler {
    /// Create an execution
    pub fn new<F>(capacity: usize, f: F) -> Scheduler
    where
        F: Fn() + Sync + Send + 'static,
    {
        Scheduler {
            kind: Fringe(fringe::Scheduler::new(capacity, f)),
        }
    }

    /// Access the execution
    pub fn with_execution<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Execution) -> R,
    {
        match KIND.with(|c| c.get()) {
            Fringe(_) => fringe::Scheduler::with_execution(f),
        }
    }

    /// Perform a context switch
    pub fn switch() {
        match KIND.with(|c| c.get()) {
            Fringe(_) => fringe::Scheduler::switch(),
        }
    }

    pub fn run(&mut self, execution: &mut Execution) {
        match self.kind {
            Fringe(ref mut v) => v.run(execution),
        }
    }
}

fn set_fringe() {
    KIND.with(|c| c.set(Fringe(())))
}
