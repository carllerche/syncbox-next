use rt::{Execution, Seed, Branch, Thread, ThreadHandle};

pub struct Scheduler {
    threads: Vec<Thread>,
    state: Execution,
}

impl Scheduler {
    /// Create an execution
    pub fn new<F: FnOnce() + 'static>(main_thread: F) -> Scheduler {
        Scheduler::with_seed(Seed::new(), main_thread)
    }

    /// Create an execution
    pub fn with_seed<F>(seed: Seed, main_thread: F) -> Scheduler
    where
        F: FnOnce() + 'static,
    {
        let mut state = Execution::new(seed);

        let th = Thread::new(state.stack(), move || {
            main_thread();
            Scheduler::thread_done();
        });

        Scheduler {
            threads: vec![th],
            state,
        }
    }

    pub fn park() {
        Execution::with(|exec| {
            exec.active_thread_mut().set_blocked();
        });

        Scheduler::branch();
    }

    pub fn acquire(th: ThreadHandle) {
        // Acquire the ordering
        Execution::with(|exec| {
            let (active, other) = exec.active_thread2_mut(th.id());

            active.join(other.causality())
        });
    }

    /// Spawn a new thread on the current execution
    pub fn spawn<F>(th: F)
    where
        F: FnOnce(ThreadHandle) + 'static,
    {
        Execution::with(|exec| {
            let thread_handle = exec.create_thread();

            let stack = exec.stack();
            let thread = Thread::new(stack, || {
                th(thread_handle);
                Scheduler::thread_done();
            });

            exec.spawn_thread(thread);
        })
    }

    /// Branch the execution
    pub fn branch() {
        Execution::with(|exec| {
            assert!(!exec.active_thread().is_critical(), "in critical section");
        });

        Thread::suspend();
    }

    fn thread_done() {
        Execution::with(|exec| {
            exec.active_thread_mut().set_terminated();
        });
    }

    pub fn run(&mut self) {
        loop {
            if self.schedule() {
                // Execution complete
                return;
            }

            self.tick();
        }
    }

    fn schedule(&mut self) -> bool {
        let start = self.state.seed.pop_front()
            .map(|branch| {
                assert!(branch.switch);
                branch.index
            })
            .unwrap_or(0);

        for (mut i, th) in self.state.threads[start..].iter().enumerate() {
            i += start;

            if th.is_runnable() {
                let last = self.state.threads[i+1..].iter()
                    .filter(|th| th.is_runnable())
                    .next()
                    .is_none();

                self.state.branches.push(Branch {
                    switch: true,
                    index: i,
                    last,
                });

                self.state.active_thread = i;

                return false;
            }
        }

        for th in self.state.threads() {
            if !th.is_terminated() {
                panic!("deadlock");
            }
        }

        true
    }

    fn tick(&mut self) {
        tick(&mut self.state, &mut self.threads);
    }

    pub(crate) fn next_seed(&mut self) -> Option<Seed> {
        self.state.next_seed()
    }
}

fn tick(execution: &mut Execution, threads: &mut Vec<Thread>) {
    let active_thread = execution.active_thread;

    let maybe_stack = execution.enter(|| {
        threads[active_thread].resume()
    });

    if let Some(stack) = maybe_stack {
        execution.stacks.push(stack);
    }

    while let Some(th) = execution.queued_spawn.pop_front() {
        let thread_id = threads.len();

        assert!(execution.threads[thread_id].is_runnable());

        threads.push(th);

        execution.active_thread = thread_id;
        tick(execution, threads);
    }
}
