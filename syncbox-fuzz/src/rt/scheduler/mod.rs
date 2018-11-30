mod gen;
mod std;

use rt::{Execution, FnBox};
use std::cell::Cell;

#[derive(Debug)]
pub struct Scheduler {
    kind: Kind<gen::Scheduler, std::Scheduler>,
}

#[derive(Copy, Clone, Debug)]
enum Kind<T = (), U = ()> {
    Generator(T),
    Thread(U),
}

use self::Kind::*;

thread_local!(static KIND: Cell<Kind> = Cell::new(Generator(())));

impl Scheduler {
    /// Create a generator based scheduler
    pub fn new_generator(capacity: usize) -> Scheduler {
        assert!(capacity > 0);
        Scheduler {
            kind: Generator(gen::Scheduler::new(capacity)),
        }
    }

    /// Create a thread based scheduler
    pub fn new_thread(capacity: usize) -> Scheduler {
        assert!(capacity > 0);
        Scheduler {
            kind: Thread(std::Scheduler::new(capacity)),
        }
    }

    /// Access the execution
    pub fn with_execution<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Execution) -> R,
    {
        match KIND.with(|c| c.get()) {
            Thread(_) => std::Scheduler::with_execution(f),
            Generator(_) => gen::Scheduler::with_execution(f),
        }
    }

    /// Perform a context switch
    pub fn switch() {
        match KIND.with(|c| c.get()) {
            Thread(_) => std::Scheduler::switch(),
            Generator(_) => gen::Scheduler::switch(),
        }
    }

    pub fn spawn(f: Box<FnBox>) {
        match KIND.with(|c| c.get()) {
            Thread(_) => std::Scheduler::spawn(f),
            Generator(_) => gen::Scheduler::spawn(f),
        }
    }

    pub fn run<F>(&mut self, execution: &mut Execution, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        match self.kind {
            Thread(ref mut v) => v.run(execution, f),
            Generator(ref mut v) => v.run(execution, f),
        }
    }
}

fn set_thread() {
    KIND.with(|c| c.set(Thread(())))
}

fn set_generator() {
    KIND.with(|c| c.set(Generator(())))
}
