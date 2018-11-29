mod fringe;
mod std;

use rt::{Execution, FnBox};
use std::cell::Cell;

#[derive(Debug)]
pub struct Scheduler {
    kind: Kind<fringe::Scheduler, std::Scheduler>,
}

#[derive(Copy, Clone, Debug)]
enum Kind<T = (), U = ()> {
    Fringe(T),
    Std(U),
}

use self::Kind::*;

thread_local!(static KIND: Cell<Kind> = Cell::new(Fringe(())));

impl Scheduler {
    /// Create an execution
    pub fn new(capacity: usize) -> Scheduler {
        assert!(capacity > 0);
        Scheduler {
            kind: Fringe(fringe::Scheduler::new(capacity)),
            // kind: Std(std::Scheduler::new(capacity)),
        }
    }

    /// Access the execution
    pub fn with_execution<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Execution) -> R,
    {
        match KIND.with(|c| c.get()) {
            Std(_) => std::Scheduler::with_execution(f),
            Fringe(_) => fringe::Scheduler::with_execution(f),
        }
    }

    /// Perform a context switch
    pub fn switch() {
        match KIND.with(|c| c.get()) {
            Std(_) => std::Scheduler::switch(),
            Fringe(_) => fringe::Scheduler::switch(),
        }
    }

    pub fn spawn(f: Box<FnBox>) {
        match KIND.with(|c| c.get()) {
            Std(_) => std::Scheduler::spawn(f),
            Fringe(_) => fringe::Scheduler::spawn(f),
        }
    }

    pub fn run<F>(&mut self, execution: &mut Execution, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        match self.kind {
            Std(ref mut v) => v.run(execution, f),
            Fringe(ref mut v) => v.run(execution, f),
        }
    }
}

fn set_std() {
    KIND.with(|c| c.set(Std(())))
}

fn set_fringe() {
    KIND.with(|c| c.set(Fringe(())))
}
