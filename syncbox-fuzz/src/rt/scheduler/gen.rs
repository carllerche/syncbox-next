use rt::{Execution, FnBox};

use generator::{self, Gn, Generator};

use std::collections::VecDeque;
use std::fmt;

pub struct Scheduler {
    /// Threads
    threads: Vec<Thread>,

    queued_spawn: VecDeque<Box<FnBox>>,
}

type Thread = Generator<'static, (), ()>;

scoped_mut_thread_local! {
    static STATE: State
}

struct State<'a> {
    execution: &'a mut Execution,
    queued_spawn: &'a mut VecDeque<Box<FnBox>>,
}

const STACK_SIZE: usize = 1 << 23;

impl Scheduler {
    /// Create an execution
    pub fn new(_capacity: usize) -> Scheduler {
        Scheduler {
            threads: vec![],
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
        generator::yield_with(());
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

        // Set the scheduler kind
        super::set_generator();

        self.threads.push(Gn::new_opt(STACK_SIZE, move || {
            f();
            done!();
        }));

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

        tick(&mut state, &mut self.threads);
    }
}

impl fmt::Debug for Scheduler {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Schedule")
            .field("threads", &self.threads)
            .finish()
    }
}

fn tick(
    state: &mut State,
    threads: &mut Vec<Thread>)
{
    let active_thread = state.execution.active_thread;

    STATE.set(unsafe { transmute_lt(state) }, || {
        threads[active_thread].resume();
    });

    while let Some(th) = state.queued_spawn.pop_front() {
        let thread_id = threads.len();

        assert!(state.execution.threads[thread_id].is_runnable());

        threads.push(Gn::new_opt(STACK_SIZE, move || {
            th.call();
            done!();
        }));
    }
}

unsafe fn transmute_lt<'a, 'b>(state: &'a mut State<'b>) -> &'a mut State<'static> {
    ::std::mem::transmute(state)
}
