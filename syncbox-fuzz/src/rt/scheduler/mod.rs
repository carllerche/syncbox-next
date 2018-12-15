#[cfg(feature = "fringe")]
mod fringe;
mod gen;
mod std;

#[cfg(not(feature = "fringe"))]
mod fringe {
    use rt::{Execution, FnBox};

    #[derive(Debug)]
    pub struct Scheduler;

    impl Scheduler {
        pub fn new(_: usize) -> Scheduler {
            unimplemented!();
        }

        /// Access the execution
        pub fn with_execution<F, R>(_: F) -> R
        where
            F: FnOnce(&mut Execution) -> R,
        {
            unimplemented!();
        }

        pub fn switch() {
            unimplemented!();
        }

        pub fn spawn(_: Box<FnBox>) {
            unimplemented!();
        }

        pub fn run<F>(&mut self, _: &mut Execution, _: F)
        where
            F: FnOnce() + Send + 'static,
        {
            unimplemented!();
        }
    }
}

use rt::{Execution, FnBox};
use std::cell::Cell;

#[derive(Debug)]
pub struct Scheduler {
    kind: Kind<gen::Scheduler, std::Scheduler, fringe::Scheduler>,
}

#[derive(Copy, Clone, Debug)]
enum Kind<T = (), U = (), V = ()> {
    Generator(T),
    Thread(U),
    Fringe(V),
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

    pub fn new_fringe(capacity: usize) -> Scheduler {
        assert!(capacity > 0);
        Scheduler {
            kind: Fringe(fringe::Scheduler::new(capacity)),
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
            Fringe(_) => fringe::Scheduler::with_execution(f),
        }
    }

    /// Perform a context switch
    pub fn switch() {
        match KIND.with(|c| c.get()) {
            Thread(_) => std::Scheduler::switch(),
            Generator(_) => gen::Scheduler::switch(),
            Fringe(_) => fringe::Scheduler::switch(),
        }
    }

    pub fn spawn(f: Box<FnBox>) {
        match KIND.with(|c| c.get()) {
            Thread(_) => std::Scheduler::spawn(f),
            Generator(_) => gen::Scheduler::spawn(f),
            Fringe(_) => fringe::Scheduler::spawn(f),
        }
    }

    pub fn run<F>(&mut self, execution: &mut Execution, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        match self.kind {
            Thread(ref mut v) => v.run(execution, f),
            Generator(ref mut v) => v.run(execution, f),
            Fringe(ref mut v) => v.run(execution, f),
        }
    }
}

fn set_thread() {
    KIND.with(|c| c.set(Thread(())))
}

fn set_generator() {
    KIND.with(|c| c.set(Generator(())))
}

#[cfg(feature = "fringe")]
fn set_fringe() {
    KIND.with(|c| c.set(Fringe(())))
}
