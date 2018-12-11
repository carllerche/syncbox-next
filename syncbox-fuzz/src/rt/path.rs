use util::CachedVec;

use std::collections::VecDeque;

/// An execution path
#[derive(Debug, Serialize, Deserialize)]
pub struct Path {
    /// Path taken
    branches: Vec<Branch>,

    /// Current execution's position in the branch index.
    ///
    /// When the execution starts, this is zero, but `branches` might not be
    /// empty.
    ///
    /// In order to perform an exhaustive search, the execution is seeded with a
    /// set of branches.
    pos: usize,

    /// Tracks threads to be scheduled
    schedules: CachedVec<Schedule>,

    /// Atomic writes
    writes: CachedVec<VecDeque<usize>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum Branch {
    Schedule(usize),
    Write(usize),
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Schedule {
    threads: Vec<Thread>,
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Thread {
    Skip,
    Pending,
    Visited,
    Terminated,
}

impl Path {
    /// New Path
    pub fn new() -> Path {
        Path {
            branches: vec![],
            pos: 0,
            schedules: CachedVec::new(),
            writes: CachedVec::new(),
        }
    }

    /// Returns the atomic write to read
    pub fn branch_write<I>(&mut self, seed: I) -> usize
    where
        I: Iterator<Item = usize>
    {
        use self::Branch::Write;

        if self.pos == self.branches.len() {
            let i = self.writes.len();

            self.writes.push(|writes| {
                writes.clear();
                writes.extend(seed);
                debug_assert!(!writes.is_empty());
            });

            self.branches.push(Branch::Write(i));
        }

        let i = match self.branches[self.pos] {
            Write(i) => i,
            _ => panic!(),
        };

        self.pos += 1;

        self.writes[i][0]
    }

    /// Returns the thread identifier to schedule
    pub fn branch_thread<I>(&mut self, seed: I) -> Option<usize>
    where
        I: Iterator<Item = Thread>
    {
        use self::Branch::Schedule;

        if self.pos == self.branches.len() {
            let i = self.schedules.len();

            self.schedules.push(|schedule| {
                schedule.threads.clear();
                schedule.threads.extend(seed);
            });

            self.branches.push(Branch::Schedule(i));
        }

        let i = match self.branches[self.pos] {
            Schedule(i) => i,
            _ => panic!(),
        };

        self.pos += 1;

        let threads = &mut self.schedules[i].threads;

        let next = threads.iter_mut()
            .enumerate()
            .find(|&(_, ref th)| th.is_pending())
            .map(|(i, _)| i)
            ;

        if next.is_none() {
            assert!({
                threads.iter().all(|th| *th == Thread::Terminated)
            }, "deadlock")
        }

        next
    }

    /// Returns `false` if there are no more paths to explore
    pub fn step(&mut self) -> bool {
        use self::Branch::*;

        self.pos = 0;

        while self.branches.len() > 0 {
            match self.branches.last().unwrap() {
                &Schedule(i) => {
                    self.schedules[i].threads.iter_mut()
                        .find(|th| th.is_pending())
                        .map(|th| *th = Thread::Visited);

                    if self.schedules[i].is_empty() {
                        self.branches.pop();
                        self.schedules.pop();
                        continue;
                    }
                }
                &Write(i) => {
                    self.writes[i].pop_front();

                    if self.writes[i].is_empty() {
                        self.branches.pop();
                        self.writes.pop();
                        continue;
                    }
                }
            }

            return true;
        }

        false
    }
}

impl Schedule {
    fn is_empty(&self) -> bool {
        !self.threads.iter()
            .any(|th| th.is_pending())
    }
}

impl Thread {
    pub fn is_pending(&self) -> bool {
        match *self {
            Thread::Pending => true,
            _ => false,
        }
    }
}

impl Default for Thread {
    fn default() -> Thread {
        Thread::Terminated
    }
}
