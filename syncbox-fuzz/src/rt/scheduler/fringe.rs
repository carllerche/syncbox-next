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

    next_thread: usize,

    queued_spawn: VecDeque<Box<FnBox>>,
}

type Thread = Generator<'static, Option<Box<FnBox>>, (), OsStack>;

struct State<'a> {
    execution: &'a mut Execution,
    queued_spawn: &'a mut VecDeque<Box<FnBox>>,
}

scoped_mut_thread_local! {
    static STATE: State
}

thread_local!(static YIELDER: Cell<*const Yielder<Option<Box<FnBox>>, ()>> = Cell::new(ptr::null()));

const STACK_SIZE: usize = 1 << 23;

impl Scheduler {
    /// Create an execution
    pub fn new(capacity: usize) -> Scheduler {
        let threads = spawn_threads(capacity);

        Scheduler {
            threads,
            next_thread: 0,
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
        assert!(suspend().is_none());
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

        // Set the scheduler kind
        super::set_fringe();

        self.next_thread = 1;
        self.threads[0].resume(Some(Box::new(f)));

        loop {
            if execution.schedule() {
                // Execution complete
                return;
            }

            self.tick(execution);
        }
    }

    fn tick(&mut self, execution: &mut Execution) {
        let mut state = State {
            execution: execution,
            queued_spawn: &mut self.queued_spawn,
        };

        tick(&mut state, &mut self.threads, &mut self.next_thread);
    }
}

pub fn suspend() -> Option<Box<FnBox>> {
    let ptr = YIELDER.with(|cell| {
        let ptr = cell.get();
        cell.set(ptr::null());
        ptr
    });

    let ret = unsafe { ptr.as_ref().unwrap().suspend(()) };

    YIELDER.with(|cell| cell.set(ptr));

    ret
}

impl fmt::Debug for Scheduler {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Scheduler")
            .finish()
    }
}

fn tick(
    state: &mut State,
    threads: &mut Vec<Thread>,
    next_thread: &mut usize)
{
    let active_thread = state.execution.threads.active;

    STATE.set(unsafe { transmute_lt(state) }, || {
        threads[active_thread].resume(None);
    });

    while let Some(th) = state.queued_spawn.pop_front() {
        let thread_id = *next_thread;
        *next_thread += 1;

        assert!(state.execution.threads.threads[thread_id].is_runnable());

        threads[thread_id].resume(Some(th));
    }
}

fn spawn_threads(n: usize) -> Vec<Thread> {
    (0..n).map(|_| {
        let stack = OsStack::new(STACK_SIZE).unwrap();

        let mut g: Thread = Generator::new(stack, move |yielder, _| {
            struct UnsetTls;

            impl Drop for UnsetTls {
                fn drop(&mut self) {
                    YIELDER.with(|cell| cell.set(ptr::null()));
                }
            }

            let _reset = UnsetTls;

            let ptr = yielder as *const _;
            YIELDER.with(|cell| cell.set(ptr));

            loop {
                let f: Option<Box<FnBox>> = suspend();
                Scheduler::switch();
                f.unwrap().call();
            }
        });
        g.resume(None);
        g
    }).collect()
}

unsafe fn transmute_lt<'a, 'b>(state: &'a mut State<'b>) -> &'a mut State<'static> {
    ::std::mem::transmute(state)
}
