use rt::thread::Thread;
use rt::vv::{Actor, VersionVec};

use fringe::OsStack;

use std::collections::VecDeque;

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
    static CURRENT_EXECUTION: Execution
}

#[derive(Debug, Eq, PartialEq, Clone)]
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

    /// Stack cache.
    pub stacks: Vec<OsStack>,
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
            stacks: seed.stacks,
        }
    }

    pub fn with<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Execution) -> R,
    {
        CURRENT_EXECUTION.with(f)
    }

    pub fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce() -> R
    {
        CURRENT_EXECUTION.set(self, f)
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

    pub fn threads(&mut self) -> &mut [ThreadState] {
        &mut self.threads
    }

    pub fn stack(&mut self) -> OsStack {
        self.stacks.pop()
            .unwrap_or_else(|| {
                OsStack::new(1 << 16).unwrap()
            })
    }

    pub(crate) fn next_seed(&mut self) -> Option<Seed> {
        use std::mem::replace;

        let mut ret: VecDeque<_> = self.branches.iter()
            .map(|b| b.clone())
            .collect();

        let stacks = replace(&mut self.stacks, vec![]);

        while !ret.is_empty() {
            let last = ret.len() - 1;

            ret[last].index += 1;

            if !ret[last].last {
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

impl ThreadState {
    fn new(causality: VersionVec) -> ThreadState {
        ThreadState {
            run: Run::Runnable,
            critical: false,
            causality,
        }
    }

    pub fn join(&mut self, other: &VersionVec) {
        self.causality.join(other);
    }

    pub fn causality(&self) -> &VersionVec {
        &self.causality
    }

    pub fn is_runnable(&self) -> bool {
        use self::Run::*;

        match self.run {
            Runnable => true,
            _ => false,
        }
    }

    pub fn set_blocked(&mut self) {
        self.run = Run::Blocked;
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

    /// Critical section, may not branch
    pub fn critical<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        struct Reset;

        impl Drop for Reset {
            fn drop(&mut self) {
                Execution::with(|exec| {
                    exec.active_thread_mut().critical = false;
                });
            }
        }

        let _reset = Reset;

        Execution::with(|exec| {
            exec.active_thread_mut().critical = true;
        });

        f()
    }
}

impl Seed {
    pub fn new() -> Seed {
        Seed {
            branches: VecDeque::new(),
            stacks: vec![],
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
    pub fn current() -> ThreadHandle {
        Execution::with(|exec| {
            ThreadHandle {
                execution: exec.id().clone(),
                thread_id: exec.active_thread,
            }
        })
    }

    pub fn id(&self) -> usize {
        self.thread_id
    }

    pub fn unpark(&self) {
        CURRENT_EXECUTION.with(|exec| {
            let th = &mut exec.threads[self.thread_id];

            println!(" ThreadHandle::unpark(); is_blocked = {:?}", th.run.is_blocked());

            if th.run.is_blocked() {
                th.run = Run::Runnable;
            }
        });
    }
}

impl Run {
    fn is_blocked(&self) -> bool {
        use self::Run::*;

        match *self {
            Blocked => true,
            _ => false,
        }
    }
}
