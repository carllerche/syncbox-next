use rt::thread::Thread;
use rt::vv::{Actor, VersionVec};

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ThreadHandle {
    execution: ExecutionId,
    thread_id: usize,
}

#[derive(Debug)]
pub struct Seed {
    branches: VecDeque<Branch>,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct ExecutionId(usize);

#[derive(Debug)]
pub struct Execution {
    /// Execution identifier
    pub id: ExecutionId,

    /// Branching start point
    pub seed: VecDeque<Branch>,

    /// Path taken
    pub branches: Vec<Branch>,

    /// State for each thread
    pub threads: Vec<ThreadState>,

    /// Currently scheduled thread
    pub active_thread: usize,

    /// Sequential consistency causality. All sequentially consistent operations synchronize with
    /// this causality.
    pub seq_cst_causality: VersionVec,

    /// Queue of spawned threads that have not yet been added to the execution.
    pub queued_spawn: VecDeque<Thread>,
}

#[derive(Debug)]
pub struct ThreadState {
    /// If the thread is runnable, blocked, or terminated.
    run: Run,

    /// True if the thread is in a critical section
    critical: bool,

    /// Tracks observed causality
    pub causality: VersionVec,
}

#[derive(Debug, Clone)]
pub struct Branch {
    /// True if a thread switch, false is an atomic load.
    pub switch: bool,

    /// Choice index
    pub index: usize,

    /// True if `index` is the final choice
    pub last: bool,
}

#[derive(Debug)]
enum Run {
    Runnable,
    Blocked,
    Yield,
    Terminated,
}

impl Execution {
    pub fn new(seed: Seed) -> Execution {
        let vv = VersionVec::root();

        Execution {
            id: ExecutionId::new(),
            seed: seed.branches,
            branches: vec![],
            threads: vec![ThreadState::new(vv)],
            active_thread: 0,
            seq_cst_causality: VersionVec::new(),
            queued_spawn: VecDeque::new(),
        }
    }

    pub fn create_thread(&mut self) -> ThreadHandle {
        let mut causality = self.active_thread_mut().causality.clone();
        let thread_id = self.threads.len();

        Actor::new(&mut causality, thread_id).inc();
        self.threads.push(ThreadState::new(causality));

        ThreadHandle {
            thread_id,
            execution: self.id.clone(),
        }
    }

    pub fn id(&self) -> &ExecutionId {
        &self.id
    }

    pub fn spawn_thread(&mut self, thread: Thread) {
        self.queued_spawn.push_back(thread);
    }

    pub fn unpark_thread(&mut self, thread_id: usize) {
        // Synchronize memory
        let (active, th) = self.active_thread2_mut(thread_id);
        th.causality.join(&active.causality);

        if th.is_blocked() || th.is_yield() {
            th.set_runnable();
        }
    }

    pub fn active_thread(&self) -> &ThreadState {
        &self.threads[self.active_thread]
    }

    pub fn active_thread_mut(&mut self) -> &mut ThreadState {
        &mut self.threads[self.active_thread]
    }

    /// Get the active thread and second thread
    pub fn active_thread2_mut(&mut self, other: usize)
        -> (&mut ThreadState, &mut ThreadState)
    {
        if other >= self.active_thread {
            let (l, r) = self.threads.split_at_mut(other);

            (&mut l[self.active_thread], &mut r[0])
        } else {
            let (l, r) = self.threads.split_at_mut(self.active_thread);

            (&mut r[0], &mut l[other])
        }
    }

    pub(crate) fn next_seed(&mut self) -> Option<Seed> {
        let mut ret: VecDeque<_> = self.branches.iter()
            .map(|b| b.clone())
            .collect();

        while !ret.is_empty() {
            let last = ret.len() - 1;

            ret[last].index += 1;

            if !ret[last].last {
                return Some(Seed {
                    branches: ret,
                });
            }

            ret.pop_back();
        }

        None
    }

    pub fn set_critical(&mut self) {
        self.active_thread_mut().critical = true;
    }

    pub fn unset_critical(&mut self) {
        self.active_thread_mut().critical = false;
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

    pub fn is_runnable(&self) -> bool {
        use self::Run::*;

        match self.run {
            Runnable => true,
            _ => false,
        }
    }

    pub fn set_runnable(&mut self) {
        self.run = Run::Runnable;
    }

    pub fn set_blocked(&mut self) {
        self.run = Run::Blocked;
    }

    fn is_blocked(&self) -> bool {
        use self::Run::*;

        match self.run {
            Blocked => true,
            _ => false,
        }
    }

    pub fn is_yield(&self) -> bool {
        use self::Run::*;

        match self.run {
            Yield => true,
            _ => false,
        }
    }

    pub fn set_yield(&mut self) {
        self.run = Run::Yield;
    }

    pub fn is_terminated(&self) -> bool {
        use self::Run::*;

        match self.run {
            Terminated => true,
            _ => false,
        }
    }

    pub fn set_terminated(&mut self) {
        self.run = Run::Terminated;
    }

    /// Returns `true` if the thread is in a critical section.
    pub fn is_critical(&self) -> bool {
        self.critical
    }
}

impl Seed {
    pub fn new() -> Seed {
        Seed {
            branches: VecDeque::new(),
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

use rt::Scheduler;

impl ThreadHandle {
    pub fn current() -> ThreadHandle {
        Scheduler::with_execution(|execution| {
            ThreadHandle {
                execution: execution.id().clone(),
                thread_id: execution.active_thread,
            }
        })
    }

    pub fn unpark(&self) {
        Scheduler::with_execution(|execution| {
            execution.unpark_thread(self.thread_id);
        });
    }
}
