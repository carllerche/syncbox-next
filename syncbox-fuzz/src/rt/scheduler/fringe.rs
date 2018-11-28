use rt::Execution;

use fringe::{
    Generator,
    OsStack,
    generator::Yielder,
};

use std::cell::Cell;
use std::ptr;
use std::sync::Arc;

pub struct Scheduler {
    /// Initialize the execution
    f: Arc<Fn() + Sync + Send>,

    /// Threads
    threads: Vec<Thread>,

    /// Re-usable stacks
    stacks: Vec<OsStack>,
}

#[derive(Debug)]
struct Thread {
    generator: Option<Generator<'static, (), (), OsStack>>,
}

scoped_mut_thread_local! {
    static CURRENT_EXECUTION: Execution
}

thread_local!(static YIELDER: Cell<*const Yielder<(), ()>> = Cell::new(ptr::null()));

impl Scheduler {
    /// Create an execution
    pub fn new<F>(capacity: usize, f: F) -> Scheduler
    where
        F: Fn() + Sync + Send + 'static,
    {
        // Create the OS stacks
        let stacks = (0..capacity)
            .map(|_| {
                OsStack::new(1 << 16).unwrap()
            })
            .collect();

        Scheduler {
            f: Arc::new(f),
            threads: vec![],
            stacks,
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
        .expect("max threads reached")
}

impl Thread {
    pub fn new<F: FnOnce() + 'static>(stack: OsStack, body: F) -> Thread {
        let generator = Generator::new(stack, move |yielder, ()| {
            struct UnsetTls;

            impl Drop for UnsetTls {
                fn drop(&mut self) {
                    YIELDER.with(|cell| cell.set(ptr::null()));
                }
            }

            let _reset = UnsetTls;

            let ptr = yielder as *const _;
            YIELDER.with(|cell| cell.set(ptr));

            body();
        });

        Thread {
            generator: Some(generator),
        }
    }

    pub fn suspend() {
        let ptr = YIELDER.with(|cell| {
            let ptr = cell.get();
            cell.set(ptr::null());
            ptr
        });

        unsafe { ptr.as_ref().unwrap().suspend(()); }

        YIELDER.with(|cell| cell.set(ptr));
    }

    pub fn resume(&mut self) -> Option<OsStack> {
        {
            let generator = self.generator
                .as_mut()
                .unwrap();

            if generator.resume(()).is_some() {
                return None;
            }
        }

        let stack = self.generator.take().unwrap().unwrap();
        Some(stack)
    }
}
