use rt::{Execution, Thread};

use fringe::OsStack;

use std::sync::Arc;

pub struct Scheduler {
    /// Initialize the execution
    f: Arc<Fn() + Sync + Send>,

    /// Threads
    threads: Vec<Thread>,

    /// State stored in TLS variable
    state: State,
}

struct State {
    /// Execution state
    execution: Execution,

    /// Re-usable stacks
    stacks: Vec<OsStack>,
}

scoped_mut_thread_local! {
    static CURRENT: State
}

impl Scheduler {
    /// Create an execution
    pub fn new<F>(f: F) -> Scheduler
    where
        F: Fn() + Sync + Send + 'static,
    {
        Scheduler {
            f: Arc::new(f),
            threads: vec![],
            state: State {
                execution: Execution::new(),
                stacks: vec![],
            },
        }
    }

    pub fn with_execution<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Execution) -> R,
    {
        CURRENT.with(|state| f(&mut state.execution))
    }

    /// Spawn a new thread on the current execution
    pub fn spawn<F>(th: F)
    where
        F: FnOnce() + 'static,
    {
        CURRENT.with(|state| {
            state.execution.create_thread();

            let stack = state.stack();
            let thread = Thread::new(stack, || {
                th();
                Scheduler::thread_done();
            });

            state.execution.spawn_thread(thread);
        })
    }

    /// Perform a context switch
    pub fn switch() {
        Thread::suspend();
    }

    fn thread_done() {
        CURRENT.with(|state| {
            state.execution.active_thread_mut().set_terminated();
        });
    }

    pub fn run(&mut self) {
        assert!(self.threads.is_empty());

        let f = self.f.clone();
        let stack = self.state.stack();

        self.threads.push(Thread::new(stack, move || {
            f();
            Scheduler::thread_done();
        }));

        loop {
            if self.state.execution.schedule() {
                // Execution complete
                return;
            }

            // Release yielded threads
            for th in self.state.execution.threads.iter_mut() {
                if th.is_yield() {
                    th.set_runnable();
                }
            }

            self.tick();
        }
    }

    pub fn step(&mut self) -> bool {
        self.threads.clear();
        self.state.execution.step()
    }

    fn tick(&mut self) {
        tick(&mut self.state, &mut self.threads);
    }
}

fn tick(state: &mut State, threads: &mut Vec<Thread>) {
    let active_thread = state.execution.active_thread;

    let maybe_stack = state.enter(|| {
        threads[active_thread].resume()
    });

    if let Some(stack) = maybe_stack {
        state.stacks.push(stack);
    }

    while let Some(th) = state.execution.queued_spawn.pop_front() {
        let thread_id = threads.len();

        assert!(state.execution.threads[thread_id].is_runnable());

        threads.push(th);

        state.execution.active_thread = thread_id;
        tick(state, threads);
    }
}

impl State {
    fn stack(&mut self) -> OsStack {
        self.stacks.pop()
            .unwrap_or_else(|| {
                OsStack::new(1 << 16).unwrap()
            })
    }

    fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce() -> R
    {
        CURRENT.set(self, f)
    }
}
