use rt::{Execution, FnBox};

use fringe::{
    Generator,
    OsStack,
    generator::Yielder,
};

use std::cell::Cell;
use std::collections::VecDeque;
use std::fmt;
use std::ptr;

pub struct Scheduler {
    /// Threads
    threads: Vec<Thread>,

    /// Re-usable stacks
    stacks: Vec<OsStack>,

    queued_spawn: VecDeque<Box<FnBox>>,
}

#[derive(Debug)]
struct Thread {
    generator: Option<Generator<'static, (), (), OsStack>>,
}

scoped_mut_thread_local! {
    static STATE: State
}

struct State<'a> {
    execution: &'a mut Execution,
    queued_spawn: &'a mut VecDeque<Box<FnBox>>,
}

thread_local!(static YIELDER: Cell<*const Yielder<(), ()>> = Cell::new(ptr::null()));

impl Scheduler {
    /// Create an execution
    pub fn new(capacity: usize) -> Scheduler {
        // Create the OS stacks
        let stacks = (0..capacity)
            .map(|_| {
                OsStack::new(1 << 16).unwrap()
            })
            .collect();

        Scheduler {
            threads: vec![],
            stacks,
            queued_spawn: VecDeque::new(),
        }
    }

    /// Access the execution
    pub fn with_execution<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Execution) -> R,
    {
        STATE.with(|state| f(state.execution))
    }

    /// Perform a context switch
    pub fn switch() {
        Thread::suspend();
    }

    pub fn spawn(f: Box<FnBox>) {
        STATE.with(|state| {
            state.queued_spawn.push_back(f);
        });
    }

    pub fn run<F>(&mut self, execution: &mut Execution, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.threads.clear();

        let stack = stack(&mut self.stacks);

        // Set the scheduler kind
        super::set_fringe();

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
        let mut state = State {
            execution: execution,
            queued_spawn: &mut self.queued_spawn,
        };

        tick(&mut state, &mut self.threads, &mut self.stacks);
    }
}

impl fmt::Debug for Scheduler {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Schedule")
            .field("threads", &self.threads)
            .field("stacks", &self.stacks)
            .finish()
    }
}

fn tick(
    state: &mut State,
    threads: &mut Vec<Thread>,
    stacks: &mut Vec<OsStack>)
{
    let active_thread = state.execution.active_thread;

    let maybe_stack = STATE.set(unsafe { transmute_lt(state) }, || {
        threads[active_thread].resume()
    });

    if let Some(stack) = maybe_stack {
        stacks.push(stack);
    }

    while let Some(th) = state.queued_spawn.pop_front() {
        let thread_id = threads.len();

        assert!(state.execution.threads[thread_id].is_runnable());

        let stack = stack(stacks);

        threads.push(Thread::new(stack, || {
            th.call();
        }));

        state.execution.active_thread = thread_id;
        tick(state, threads, stacks);
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

unsafe fn transmute_lt<'a, 'b>(state: &'a mut State<'b>) -> &'a mut State<'static> {
    ::std::mem::transmute(state)
}
