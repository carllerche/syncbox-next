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

    /// Path taken
    pub branches: Vec<Branch>,

    /// Current execution's position in the branch index.
    ///
    /// When the execution starts, this is zero, but `branches` might not be
    /// empty.
    ///
    /// In order to perform an exhaustive search, the execution is seeded with a
    /// set of branches.
    pub branches_pos: usize,

    /// State for each thread
    pub threads: Vec<ThreadState>,

    /// Currently scheduled thread
    pub active_thread: usize,

    /// Sequential consistency causality. All sequentially consistent operations
    /// synchronize with this causality.
    pub seq_cst_causality: VersionVec,

    /// Threads that have been spawned and are pending their first schedule.
    ///
    /// The first time a thread is scheduled does not factor into the branching.
    pub spawned: VecDeque<usize>,

    /// Log execution output to STDOUT
    pub log: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Create a new execution.
    ///
    /// This is only called at the start of a fuzz run. The same instance is
    /// reused across permutations.
    pub fn new() -> Execution {
        let vv = VersionVec::root();

        Execution {
            id: ExecutionId::new(),
            branches: vec![],
            branches_pos: 0,
            threads: vec![ThreadState::new(vv)],
            active_thread: 0,
            seq_cst_causality: VersionVec::new(),
            spawned: VecDeque::new(),
            log: false,
        }
    }

    /// Create state to track a new thread
    pub fn create_thread(&mut self) -> ThreadHandle {
        let mut causality = self.active_thread_mut().causality.clone();
        let thread_id = self.threads.len();

        Actor::new(&mut causality, thread_id).inc();
        self.threads.push(ThreadState::new(causality));
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
        if thread_id == self.active_thread {
            return;
        }

        // Synchronize memory
        let (active, th) = self.active_thread2_mut(thread_id);
        th.causality.join(&active.causality);

        if th.is_blocked() || th.is_yield() {
            th.set_runnable();
        }
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

    /// Resets the execution state for the next execution run
    pub fn step(&mut self) -> bool {
        self.id = ExecutionId::new();
        self.branches_pos = 0;
        self.threads.clear();
        self.spawned.clear();
        self.active_thread = 0;
        self.seq_cst_causality = VersionVec::new();

        self.threads.push(ThreadState::new(VersionVec::root()));

        while !self.branches.is_empty() {
            let last = self.branches.len() - 1;

            if !self.branches[last].last {
                self.branches[last].index += 1;
                return true;
            }

            self.branches.pop();
        }

        false
    }

    pub fn schedule(&mut self) -> bool {
        let ret = self.schedule2();

        if self.log {
            println!("===== TH {} =====", self.active_thread);
        }

        ret
    }

    fn schedule2(&mut self) -> bool {
        // Threads that are spawned but have not yet executed get scheduled
        // first. These first executions do not factor in the run permutations.
        //
        if let Some(i) = self.spawned.pop_front() {
            self.active_thread = i;
            return false;
        }

        let ret = self.schedule3();

        // Release yielded threads
        for th in self.threads.iter_mut() {
            if th.is_yield() {
                th.set_runnable();
            }
        }

        ret
    }

    /// Schedule a thread for execution.
    fn schedule3(&mut self) -> bool {
        // Find where we currently are in the run
        let (start, push) = self.branches.get(self.branches_pos)
            .map(|branch| {
                assert!(branch.switch);
                (branch.index, false)
            })
            .unwrap_or((0, true));

        debug_assert!(start < self.threads.len());

        let mut yielded = None;

        for (mut i, th) in self.threads[start..].iter().enumerate() {
            i += start;

            if th.is_runnable() {
                // TODO: Maybe this shouldn't be computed every time.
                let last = self.threads[i+1..].iter()
                    .filter(|th| th.is_runnable())
                    .next()
                    .is_none();

                if push {
                    // Max execution depth
                    assert!(self.branches.len() <= 1_000);

                    self.branches.push(Branch {
                        switch: true,
                        index: i,
                        last,
                    });
                } else {
                    self.branches[self.branches_pos].last = last;
                }

                self.active_thread = i;
                self.branches_pos += 1;

                return false;
            } else if th.is_yield() {
                if yielded.is_none() {
                    yielded = Some(i);
                }
            }
        }

        if let Some(i) = yielded {
            if push {
                self.branches.push(Branch {
                    switch: true,
                    index: i,
                    last: true,
                });
            }

            self.active_thread = i;
            self.branches_pos += 1;

            return false;
        }

        for th in self.threads.iter() {
            if !th.is_terminated() {
                panic!("deadlock; threads={:?}", self.threads);
            }
        }

        true
    }

    pub fn set_critical(&mut self) {
        self.active_thread_mut().critical = true;
    }

    pub fn unset_critical(&mut self) {
        self.active_thread_mut().critical = false;
    }
}

impl fmt::Debug for Execution {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Execution")
            .field("id", &self.id)
            .field("branches", &self.branches)
            .field("branches_pos", &self.branches_pos)
            .field("threads", &self.threads)
            .field("active_thread", &self.active_thread)
            .field("seq_cst_causality", &self.seq_cst_causality)
            .finish()
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
                thread_id: execution.active_thread,
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
            execution.active_thread_mut().notified = true;
            execution.unpark_thread(self.thread_id);
        });
    }
}
