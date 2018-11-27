use rt::{Execution, Thread};

use fringe::OsStack;

use std::sync::Arc;

pub struct Scheduler {
    /// Initialize the execution
    f: Arc<Fn() + Sync + Send>,

    /// Threads
    threads: Vec<Thread>,

    /// Re-usable stacks
    stacks: Vec<OsStack>,
}

scoped_mut_thread_local! {
    static CURRENT_EXECUTION: Execution
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
            stacks: vec![],
        }
    }

    /// Access the execution
    pub fn with_execution<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Execution) -> R,
    {
        CURRENT_EXECUTION.with(|execution| f(execution))
    }

    /// Perform a context switch
    pub fn switch() {
        Thread::suspend();
    }

    pub fn run(&mut self, execution: &mut Execution) {
        self.threads.clear();

        let f = self.f.clone();
        let stack = stack(&mut self.stacks);

        self.threads.push(Thread::new(stack, move || {
            f();
        }));

        loop {
            if execution.schedule() {
                // Execution complete
                return;
            }

            // Release yielded threads
            for th in execution.threads.iter_mut() {
                if th.is_yield() {
                    th.set_runnable();
                }
            }

            self.tick(execution);
        }
    }

    fn tick(&mut self, execution: &mut Execution) {
        tick(execution, &mut self.threads, &mut self.stacks);
    }
}

fn tick(
    execution: &mut Execution,
    threads: &mut Vec<Thread>,
    stacks: &mut Vec<OsStack>)
{
    let active_thread = execution.active_thread;

    let maybe_stack = CURRENT_EXECUTION.set(execution, || {
        threads[active_thread].resume()
    });

    if let Some(stack) = maybe_stack {
        stacks.push(stack);
    }

    while let Some(th) = execution.queued_spawn.pop_front() {
        let thread_id = threads.len();

        assert!(execution.threads[thread_id].is_runnable());

        let stack = stack(stacks);

        threads.push(Thread::new(stack, || {
            th.call();
        }));

        execution.active_thread = thread_id;
        tick(execution, threads, stacks);
    }
}

fn stack(stacks: &mut Vec<OsStack>) -> OsStack {
    stacks.pop()
        .unwrap_or_else(|| {
            OsStack::new(1 << 16).unwrap()
        })
}
