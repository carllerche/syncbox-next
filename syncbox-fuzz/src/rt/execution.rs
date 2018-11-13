use rt::thread::Thread;
use rt::vv::VersionVec;

use fringe::OsStack;

use std::collections::VecDeque;
use std::mem::replace;

pub struct Execution {
    threads: Vec<Thread>,
    state: ExecutionState,
}

#[derive(Debug, Clone)]
pub struct ThreadHandle {
    execution: ExecutionId,
    thread_id: usize,
}

#[derive(Debug)]
pub struct Seed {
    branches: VecDeque<Branch>,
    stacks: Vec<OsStack>,
}

scoped_mut_thread_local! {
    static CURRENT_EXECUTION: ExecutionState
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct ExecutionId(usize);

#[derive(Debug)]
struct ExecutionState {
    id: ExecutionId,
    seed: VecDeque<Branch>,
    branches: Vec<Branch>,
    threads: Vec<ThreadState>,
    active_thread: usize,
    queued_spawn: VecDeque<Thread>,
    stacks: Vec<OsStack>,
}

#[derive(Debug)]
struct ThreadState {
    /// If the thread is runnable, blocked, or terminated.
    run: Run,

    /// True if the thread is in a critical section
    critical: bool,

    /// Tracks observed causality
    causality: VersionVec,
}

#[derive(Debug, Clone)]
struct Branch {
    index: usize,
    rem: usize,
}

#[derive(Debug)]
enum Run {
    Runnable,
    Blocked,
    Terminated,
}

impl Execution {
    /// Create an execution
    pub fn new<F: FnOnce() + 'static>(main_thread: F) -> Execution {
        let seed = Seed {
            branches: VecDeque::new(),
            stacks: vec![],
        };

        Execution::with_seed(seed, main_thread)
    }

    /// Create an execution
    pub fn with_seed<F>(seed: Seed, main_thread: F) -> Execution
    where
        F: FnOnce() + 'static,
    {
        let mut vv = VersionVec::new();
        vv.inc(0);

        let mut state = ExecutionState {
            id: ExecutionId::new(),
            seed: seed.branches,
            branches: vec![],
            threads: vec![ThreadState::new(vv)],
            active_thread: 0,
            queued_spawn: VecDeque::new(),
            stacks: seed.stacks,
        };

        let th = Thread::new(state.stack(), move || {
            main_thread();
            Execution::thread_done();
        });

        Execution {
            threads: vec![th],
            state,
        }
    }

    pub fn current() -> ThreadHandle {
        CURRENT_EXECUTION.with(|exec| {
            ThreadHandle {
                execution: exec.id.clone(),
                thread_id: exec.active_thread,
            }
        })
    }

    pub fn park() {
        CURRENT_EXECUTION.with(|exec| {
            exec.active_thread_mut().run =
                Run::Blocked;
        });

        Execution::branch();
    }

    pub fn acquire(th: ThreadHandle) {
        // Acquire the ordering
        CURRENT_EXECUTION.with(|exec| {
            if th.thread_id >= exec.active_thread {
                let (l, r) = exec.threads.split_at_mut(th.thread_id);

                l[exec.active_thread].causality.join(
                    &r[0].causality);
            } else {
                let (l, r) = exec.threads.split_at_mut(exec.active_thread);

                r[0].causality.join(
                    &l[th.thread_id].causality);
            }
        });
    }

    /// Spawn a new thread on the current execution
    pub fn spawn<F>(th: F)
    where
        F: FnOnce(ThreadHandle) + 'static,
    {
        CURRENT_EXECUTION.with(|exec| {
            let mut causality = exec.threads[exec.active_thread]
                .causality.clone();

            let thread_id = exec.threads.len();

            let thread_handle = ThreadHandle {
                thread_id,
                execution: exec.id.clone(),
            };

            let stack = exec.stack();
            let th = Thread::new(stack, || {
                th(thread_handle);
                Execution::thread_done();
            });

            // Increment the causality
            causality.inc(thread_id);

            // Push the thread state
            exec.threads.push(ThreadState::new(causality));

            // Queue the new thread
            exec.queued_spawn
                .push_back(th);
        })
    }

    /// Branch the execution
    pub fn branch() {
        CURRENT_EXECUTION.with(|exec| {
            assert!(!exec.active_thread().critical, "in critical section");
        });

        Thread::suspend();
    }

    /// Critical section, may not branch
    pub fn critical<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        struct Reset;

        impl Drop for Reset {
            fn drop(&mut self) {
                CURRENT_EXECUTION.with(|exec| {
                    exec.active_thread_mut().critical = false;
                });
            }
        }

        let _reset = Reset;

        CURRENT_EXECUTION.with(|exec| {
            exec.active_thread_mut().critical = true;
        });

        f()
    }

    fn thread_done() {
        CURRENT_EXECUTION.with(|exec| {
            let th = &mut exec.threads[exec.active_thread];
            th.run = Run::Terminated;
        });
    }

    pub fn with_version<F, R>(f: F) -> R
    where
        F: FnOnce(&mut VersionVec, usize) -> R
    {
        CURRENT_EXECUTION.with(|exec| {
            let id = exec.active_thread;
            f(&mut exec.active_thread_mut().causality, id)
        })
    }

    pub fn run(&mut self) {
        loop {
            if self.schedule() {
                // Execution complete
                return;
            }

            self.state.tick(&mut self.threads);
        }
    }

    fn schedule(&mut self) -> bool {
        let start = self.state.seed.pop_front()
            .map(|branch| branch.index)
            .unwrap_or(0);

        for (mut i, th) in self.state.threads[start..].iter().enumerate() {
            i += start;

            if th.run.is_runnable() {
                let rem = self.state.threads[i+1..].iter()
                    .filter(|th| th.run.is_runnable())
                    .count();

                self.state.branches.push(Branch {
                    index: i,
                    rem,
                });

                self.state.active_thread = i;

                return false;
            }
        }

        for th in &self.state.threads {
            if !th.run.is_terminated() {
                panic!("deadlock");
            }
        }

        true
    }

    pub(crate) fn next_seed(&mut self) -> Option<Seed> {
        let mut ret: VecDeque<_> = self.state.branches.iter()
            .map(|b| b.clone())
            .collect();

        let stacks = replace(&mut self.state.stacks, vec![]);

        while !ret.is_empty() {
            let last = ret.len() - 1;

            ret[last].index += 1;

            if ret[last].rem > 0 {
                return Some(Seed {
                    branches: ret,
                    stacks,
                });
            }

            ret.pop_back();
        }

        None
    }
}

impl ExecutionState {
    fn active_thread(&self) -> &ThreadState {
        &self.threads[self.active_thread]
    }

    fn active_thread_mut(&mut self) -> &mut ThreadState {
        &mut self.threads[self.active_thread]
    }

    fn tick(&mut self, threads: &mut Vec<Thread>) {
        let active_thread = self.active_thread;

        let maybe_stack = CURRENT_EXECUTION.set(self, || {
            threads[active_thread].resume()
        });

        if let Some(stack) = maybe_stack {
            self.stacks.push(stack);
        }

        while let Some(th) = self.queued_spawn.pop_front() {
            let thread_id = threads.len();

            assert!(self.threads[thread_id].run.is_runnable());

            threads.push(th);

            self.active_thread = thread_id;
            self.tick(threads);
        }
    }

    fn stack(&mut self) -> OsStack {
        self.stacks.pop()
            .unwrap_or_else(|| {
                OsStack::new(1 << 16).unwrap()
            })
    }
}

impl ThreadState {
    fn new(causality: VersionVec) -> ThreadState {
        ThreadState {
            run: Run::Runnable,
            critical: false,
            causality,
        }
    }
}

impl ExecutionId {
    // Generate a new unique execution ID.
    fn new() -> ExecutionId {
        use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT};
        use std::sync::atomic::Ordering::Relaxed;
        use std::usize;

        // We never call `GUARD.init()`, so it is UB to attempt to
        // acquire this mutex reentrantly!
        static COUNTER: AtomicUsize = ATOMIC_USIZE_INIT;

        let mut curr = COUNTER.load(Relaxed);

        loop {
            if curr == usize::MAX {
                panic!("failed to generate unique execution ID: bitspace exhausted");
            }

            match COUNTER.compare_exchange(curr, curr + 1, Relaxed, Relaxed) {
                Ok(_) => return ExecutionId(curr),
                Err(actual) => {
                    curr = actual;
                }
            }
        }
    }
}

impl ThreadHandle {
    pub fn unpark(&self) {
        CURRENT_EXECUTION.with(|exec| {
            assert!(exec.threads[self.thread_id].run.is_blocked());
            exec.threads[self.thread_id].run = Run::Runnable;
        });
    }
}

impl Run {
    fn is_runnable(&self) -> bool {
        use self::Run::*;

        match *self {
            Runnable => true,
            _ => false,
        }
    }

    fn is_blocked(&self) -> bool {
        use self::Run::*;

        match *self {
            Blocked => true,
            _ => false,
        }
    }

    fn is_terminated(&self) -> bool {
        use self::Run::*;

        match *self {
            Terminated => true,
            _ => false,
        }
    }
}
