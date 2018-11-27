use rt::{Execution, Seed, Branch, Thread};

use fringe::OsStack;

pub struct Scheduler {
    threads: Vec<Thread>,
    state: State,
}

struct State {
    execution: Execution,
    stacks: Vec<OsStack>,
}

scoped_mut_thread_local! {
    static CURRENT: State
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

impl Scheduler {
    /// Create an execution
    pub fn new<F: FnOnce() + 'static>(main_thread: F) -> Scheduler {
        Scheduler::with_seed(Seed::new(), main_thread)
    }

    pub fn with_execution<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Execution) -> R,
    {
        CURRENT.with(|state| f(&mut state.execution))
    }

    /// Create an execution
    pub fn with_seed<F>(seed: Seed, main_thread: F) -> Scheduler
    where
        F: FnOnce() + 'static,
    {
        let mut state = State {
            execution: Execution::new(seed),
            stacks: vec![],
        };

        let th = Thread::new(state.stack(), move || {
            main_thread();
            Scheduler::thread_done();
        });

        Scheduler {
            threads: vec![th],
            state,
        }
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

    /// Branch the execution
    pub fn branch(is_yield: bool) {
        CURRENT.with(|state| {
            assert!(!state.execution.active_thread().is_critical(), "in critical section");

            if is_yield {
                state.execution.active_thread_mut().set_yield();
            }

        });

        Thread::suspend();
    }

    fn thread_done() {
        CURRENT.with(|state| {
            state.execution.active_thread_mut().set_terminated();
        });
    }

    pub fn run(&mut self) {
        loop {
            if self.schedule() {
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

    fn schedule(&mut self) -> bool {
        let start = self.state.execution.seed.pop_front()
            .map(|branch| {
                assert!(branch.switch);
                branch.index
            })
            .unwrap_or(0);

        for (mut i, th) in self.state.execution.threads[start..].iter().enumerate() {
            i += start;

            if th.is_runnable() {
                let last = self.state.execution.threads[i+1..].iter()
                    .filter(|th| th.is_runnable())
                    .next()
                    .is_none();

                // Max execution depth
                assert!(self.state.execution.branches.len() <= 1_000_000);

                self.state.execution.branches.push(Branch {
                    switch: true,
                    index: i,
                    last,
                });

                self.state.execution.active_thread = i;

                return false;
            }
        }

        for th in self.state.execution.threads.iter() {
            if !th.is_terminated() {
                panic!("deadlock; threads={:?}", self.state.execution.threads);
            }
        }

        true
    }

    fn tick(&mut self) {
        tick(&mut self.state, &mut self.threads);
    }

    pub(crate) fn next_seed(&mut self) -> Option<Seed> {
        self.state.execution.next_seed()
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
