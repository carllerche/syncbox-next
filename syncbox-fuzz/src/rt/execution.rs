use rt::thread::Thread;
use rt::vv::VersionVec;

use fringe::OsStack;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::mem::replace;

pub struct Execution {
    threads: Vec<Thread>,
    state: ExecutionState,
}

pub struct JoinHandle {
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

    /// Thread joining this one
    waiter: Option<usize>,

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

    /// Spawn a new thread on the current execution
    pub fn spawn<F: FnOnce() + 'static>(th: F) -> JoinHandle {
        CURRENT_EXECUTION.with(|exec| {
            let mut causality = exec.threads[exec.active_thread]
                .causality.clone();

            let stack = exec.stack();
            let th = Thread::new(stack, th);

            let thread_id = exec.threads.len();

            // Increment the causality
            causality.inc(thread_id);

            // Push the thread state
            exec.threads.push(ThreadState::new(causality));

            // Queue the new thread
            exec.queued_spawn
                .push_back(th);

            JoinHandle {
                execution: exec.id.clone(),
                thread_id,
            }
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

    pub fn thread_done() {
        CURRENT_EXECUTION.with(|exec| {
            let waiter = {
                let th = &mut exec.threads[exec.active_thread];
                th.run = Run::Terminated;
                th.waiter.take()
            };

            if let Some(waiter) = waiter {
                let th = &mut exec.threads[waiter];

                if th.run.is_blocked() {
                    th.run = Run::Runnable;
                }
            }
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

    /// Returns `true` if `thread_id` has completed
    fn wait_on(&mut self, thread_id: usize) -> bool {
        let me = self.active_thread;

        {
            let other = &mut self.threads[thread_id];

            if other.run.is_terminated() {
                return true;
            }

            other.waiter = Some(me);
        }

        // Set current run state to blocked
        self.threads[me].run =
            Run::Blocked;

        false
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
            waiter: None,
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

impl JoinHandle {
    pub fn wait(&self) {
        let unblocked = CURRENT_EXECUTION.with(|exec| {
            assert_eq!(exec.id, self.execution);
            exec.wait_on(self.thread_id)
        });

        if !unblocked {
            Execution::branch();
        }

        // Acquire the ordering
        CURRENT_EXECUTION.with(|exec| {
            if self.thread_id >= exec.active_thread {
                let (l, r) = exec.threads.split_at_mut(self.thread_id);

                l[exec.active_thread].causality.join(
                    &r[0].causality);
            } else {
                let (l, r) = exec.threads.split_at_mut(exec.active_thread);

                r[0].causality.join(
                    &l[self.thread_id].causality);
            }
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

/*
use rt::Thread;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread::{self, Thread as StdThread};

/// Initialize a new execution, returning the root thread.
pub fn create(prev: Option<Execution>) -> (Execution, Thread) {
    let threads = vec![ThreadEntry::new()];

    let seed = prev
        .map(|execution| {
            let seed = execution.next_seed();
            assert!(!seed.is_empty());
            seed
        })
        .unwrap_or(VecDeque::new());

    let inner = Arc::new(Mutex::new(Inner {
        seed,
        branches: vec![],
        threads,
        executing: 0,
    }));

    let execution = Execution {
        inner: inner.clone(),
    };

    let thread = Thread::new(Handle {
        inner,
        index: 0,
    });

    (execution, thread)
}

#[derive(Debug)]
pub struct Execution {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug, Clone)]
pub struct Handle {
    inner: Arc<Mutex<Inner>>,
    index: usize,
}

#[derive(Debug)]
struct Inner {
    seed: VecDeque<Branch>,

    branches: Vec<Branch>,

    /// State for each thread
    threads: Vec<ThreadEntry>,

    /// Index of the executing threads
    executing: usize,
}

#[derive(Debug)]
struct ThreadEntry {
    /// `true` if the thread is in a runnable state.
    runnable: bool,

    /// std thread to notify when lock acquired.
    std: Option<StdThread>,
}

#[derive(Debug, Clone)]
pub(crate) struct Branch {
    index: usize,
    rem: usize,
}

// ===== impl Execution =====

impl Execution {
    pub(crate) fn next_seed(&self) -> VecDeque<Branch> {
        let inner = self.inner.lock().unwrap();

        let mut ret: VecDeque<_> = inner.branches.iter()
            .map(|b| b.clone())
            .collect();

        while !ret.is_empty() {
            let last = ret.len() - 1;

            ret[last].index += 1;

            if ret[last].rem > 0 {
                return ret;
            }

            ret.pop_back();
        }

        ret
    }
}

// ===== impl Handle =====

impl Handle {
    pub fn new_thread(&self) -> Thread {
        let index = {
            let mut lock = self.inner.lock().unwrap();
            let index = lock.threads.len();

            lock.threads.push(ThreadEntry::new());

            index
        };

        let handle = Handle {
            inner: self.inner.clone(),
            index,
        };

        Thread::new(handle)
    }

    /// Enter the thread context
    pub fn enter(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.threads[self.index].std = Some(thread::current());
        }

        self.wait_for_lock();
    }

    /// Branch execution
    pub fn branch(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.schedule();
        }

        self.wait_for_lock()
    }

    /// Wait for the execution lock
    pub fn wait_for_lock(&self) {
        loop {
            {
                let inner = self.inner.lock().unwrap();
                if inner.executing == self.index {
                    return;
                }
            }

            thread::park();
        }
    }

    /// Mark the thread as blocked
    pub fn blocked(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.threads[self.index].runnable = false;
    }

    /// Mark the thread as runnable
    pub fn unblocked(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.threads[self.index].runnable = true;
    }

    /// Terminate the thread
    pub fn terminate(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.threads[self.index].runnable = false;

        inner.schedule();
    }
}

// ===== impl Inner =====

impl Inner {
    /// Schedule a thread for execution
    fn schedule(&mut self) {
        let start = self.seed.pop_front()
            .map(|branch| branch.index)
            .unwrap_or(0);

        for (mut i, th) in self.threads[start..].iter().enumerate() {
            i += start;

            if th.runnable {
                let rem = self.threads[i+1..].iter()
                    .filter(|th| th.runnable)
                    .count();

                self.branches.push(Branch {
                    index: i,
                    rem,
                });

                self.executing = i;

                if let Some(ref th) = th.std {
                    th.unpark();
                }

                return;
            }
        }
    }
}

// ===== impl ThreadEntry =====

impl ThreadEntry {
    fn new() -> ThreadEntry {
        ThreadEntry {
            runnable: true,
            std: None,
        }
    }
}
*/
