use rt::Path;
use rt::arena::Arena;
use rt::vv::{Actor, VersionVec};

use std::collections::VecDeque;
use std::fmt;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ThreadHandle {
    execution: ExecutionId,
    thread_id: usize,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct ExecutionId(usize);

pub struct Execution {
    /// Execution identifier
    pub id: ExecutionId,

    /// Execution path taken
    pub path: Path,

    pub threads: ThreadSet,

    /// Sequential consistency causality. All sequentially consistent operations
    /// synchronize with this causality.
    pub seq_cst_causality: VersionVec,

    /// Threads that have been spawned and are pending their first schedule.
    ///
    /// The first time a thread is scheduled does not factor into the branching.
    pub spawned: VecDeque<usize>,

    /// Arena allocator
    pub arena: Arena,

    /// Maximum number of concurrent threads
    pub max_threads: usize,

    pub max_history: usize,

    /// Log execution output to STDOUT
    pub log: bool,
}

#[derive(Debug)]
pub struct ThreadSet {
    pub threads: Vec<ThreadState>,

    /// Currently scheduled thread
    pub active: usize,
}

#[derive(Debug)]
pub struct ThreadState {
    /// If the thread is runnable, blocked, or terminated.
    run: Run,

    /// True if the thread is in a critical section
    critical: bool,

    /// Tracks observed causality
    pub causality: VersionVec,

    /// Tracks a future's `Task::notify` flag
    pub notified: bool,
}

#[derive(Debug)]
enum Run {
    Runnable,
    Blocked,
    Yield,
    Terminated,
}

impl Execution {
    /// Create a new execution.
    ///
    /// This is only called at the start of a fuzz run. The same instance is
    /// reused across permutations.
    pub fn new(max_threads: usize, max_memory: usize) -> Execution {
        let mut arena = Arena::with_capacity(max_memory);
        let root_vv = VersionVec::root(max_threads, &mut arena);
        let seq_cst_causality = VersionVec::new(max_threads, &mut arena);

        Execution {
            id: ExecutionId::new(),
            path: Path::new(),
            threads: ThreadSet {
                threads: vec![ThreadState::new(root_vv)],
                active: 0,
            },
            seq_cst_causality,
            spawned: VecDeque::new(),
            arena: Arena::with_capacity(max_memory),
            max_threads,
            max_history: 7,
            log: false,
        }
    }

    /// Create state to track a new thread
    pub fn create_thread(&mut self) -> ThreadHandle {
        let mut causality = self.threads.active_mut().causality.clone_with(&mut self.arena);
        let thread_id = self.threads.threads.len();

        causality[thread_id] += 1;

        self.threads.threads.push(ThreadState::new(causality));
        self.spawned.push_back(thread_id);

        ThreadHandle {
            thread_id,
            execution: self.id.clone(),
        }
    }

    pub fn id(&self) -> &ExecutionId {
        &self.id
    }

    pub fn unpark_thread(&mut self, thread_id: usize) {
        if thread_id == self.threads.active {
            return;
        }

        // Synchronize memory
        let (active, th) = self.threads.active2_mut(thread_id);
        th.causality.join(&active.causality);

        if th.is_blocked() || th.is_yield() {
            th.set_runnable();
        }
    }

    /// Resets the execution state for the next execution run
    pub fn step(self) -> Option<Self> {
        let max_threads = self.max_threads;
        let max_history = self.max_history;
        let log = self.log;
        let mut arena = self.arena;
        let mut path = self.path;
        let mut spawned = self.spawned;

        let mut threads = self.threads;
        threads.threads.clear();
        threads.active = 0;

        spawned.clear();

        // Force dropping the rest of the fields here
        drop(self.seq_cst_causality);

        arena.clear();

        if !path.step() {
            return None;
        }

        let root_vv = VersionVec::root(max_threads, &mut arena);
        threads.threads.push(ThreadState::new(root_vv));

        let seq_cst_causality = VersionVec::new(max_threads, &mut arena);

        Some(Execution {
            id: ExecutionId::new(),
            path,
            threads,
            seq_cst_causality,
            spawned,
            arena,
            max_threads,
            max_history,
            log,
        })
    }

    pub fn schedule(&mut self) -> bool {
        let ret = self.schedule2();

        if self.log {
            println!("===== TH {} =====", self.threads.active);
        }

        ret
    }

    fn schedule2(&mut self) -> bool {
        // Threads that are spawned but have not yet executed get scheduled
        // first. These first executions do not factor in the run permutations.
        //
        if let Some(i) = self.spawned.pop_front() {
            self.threads.active = i;
            return false;
        }

        let ret = self.schedule3();

        // Release yielded threads
        for th in self.threads.threads.iter_mut() {
            if th.is_yield() {
                th.set_runnable();
            }
        }

        ret
    }

    /// Called by `schedule2`
    fn schedule3(&mut self) -> bool {
        use rt::path::Thread;

        let next = self.path.branch_thread({
            self.threads.threads.iter().map(|th| {
                if th.is_runnable() {
                    Thread::Pending
                } else if th.is_terminated() {
                    Thread::Terminated
                } else {
                    Thread::Skip
                }
            })
        });

        if let Some(th) = next {
            self.threads.active = th;
            false
        } else {
            true
        }
    }

    pub fn set_critical(&mut self) {
        self.threads.active_mut().critical = true;
    }

    pub fn unset_critical(&mut self) {
        self.threads.active_mut().critical = false;
    }

    /// Insert a point of sequential consistency
    pub fn seq_cst(&mut self) {
        self.threads.actor().join(&self.seq_cst_causality);
        self.seq_cst_causality.join(self.threads.actor().happens_before());
    }
}

impl fmt::Debug for Execution {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Execution")
            .field("id", &self.id)
            .field("path", &self.path)
            .field("threads", &self.threads.threads)
            .field("active_thread", &self.threads.active)
            .field("seq_cst_causality", &self.seq_cst_causality)
            .finish()
    }
}

impl ThreadSet {
    pub fn active_mut(&mut self) -> &mut ThreadState {
        &mut self.threads[self.active]
    }

    /// Get the active thread and second thread
    pub fn active2_mut(&mut self, other: usize)
        -> (&mut ThreadState, &mut ThreadState)
    {
        if other >= self.active {
            let (l, r) = self.threads.split_at_mut(other);

            (&mut l[self.active], &mut r[0])
        } else {
            let (l, r) = self.threads.split_at_mut(self.active);

            (&mut r[0], &mut l[other])
        }
    }

    pub fn actor(&mut self) -> Actor {
        Actor::new(self)
    }
}

impl ThreadState {
    fn new(causality: VersionVec) -> ThreadState {
        ThreadState {
            run: Run::Runnable,
            critical: false,
            causality,
            notified: false,
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
                thread_id: execution.threads.active,
            }
        })
    }

    pub fn unpark(&self) {
        Scheduler::with_execution(|execution| {
            execution.unpark_thread(self.thread_id);
        });
    }

    #[cfg(feature = "futures")]
    pub fn future_notify(&self) {
        Scheduler::with_execution(|execution| {
            execution.threads.active_mut().notified = true;
            execution.unpark_thread(self.thread_id);
        });
    }
}
