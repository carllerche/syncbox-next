#![allow(warnings)]

use rt::{Execution, FnBox};
use std::sync::Mutex;

#[derive(Debug)]
pub struct Scheduler {
}

scoped_mut_thread_local! {
    static STATE: State
}

struct State<'a> {
    execution: Mutex<&'a mut Execution>,
}

impl Scheduler {
    /// Create an execution
    pub fn new<F>(capacity: usize, f: F) -> Scheduler
    where
        F: Fn() + Sync + Send + 'static,
    {
        unimplemented!();
    }

    /// Access the execution
    pub fn with_execution<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Execution) -> R,
    {
        STATE.with(|state| {
            let mut execution = state.execution.lock().unwrap();
            f(&mut **execution)
        })
    }

    /// Perform a context switch
    pub fn switch() {
        unimplemented!();
    }

    pub fn spawn(f: Box<FnBox>) {
        unimplemented!();
    }

    pub fn run<F>(&mut self, execution: &mut Execution, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        unimplemented!();
    }
}
