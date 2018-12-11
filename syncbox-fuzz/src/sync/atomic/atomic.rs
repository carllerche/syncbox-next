use rt::{self, Actor, Execution, Synchronize};

use std::cell::RefCell;
use std::sync::atomic::Ordering;

/// An atomic value
#[derive(Debug)]
pub struct Atomic<T> {
    writes: RefCell<Vec<Write<T>>>,
}

#[derive(Debug)]
struct Write<T> {
    /// The written value
    value: T,

    /// Manages causality transfers between threads
    sync: Synchronize,

    /// Tracks when each thread first saw value
    first_seen: FirstSeen,

    /// True when the write was done with `SeqCst` ordering
    seq_cst: bool,
}

#[derive(Debug)]
struct FirstSeen(Vec<Option<usize>>);

impl<T> Atomic<T>
where
    T: Copy + PartialEq,
{
    pub fn new(value: T) -> Atomic<T> {
        rt::execution(|execution| {
            let writes = vec![Write {
                value,
                sync: Synchronize::new(execution.max_threads, &mut execution.arena),
                first_seen: FirstSeen::new(execution),
                seq_cst: false,
            }];

            Atomic {
                writes: RefCell::new(writes),
            }
        })
    }

    pub fn load(&self, order: Ordering) -> T {
        rt::branch();
        let mut writes = self.writes.borrow_mut();

        synchronize(|execution| {
            // Pick a write that satisfies causality and specified ordering.
            let write = pick_write(&mut writes[..], execution, order);
            write.first_seen.touch(&execution.threads.actor());
            write.sync.sync_read(execution, order);
            write.value
        })
    }

    pub fn store(&self, val: T, order: Ordering) {
        rt::branch();
        let mut writes = self.writes.borrow_mut();

        synchronize(|execution| {
            do_write(val, &mut *writes, execution, order);
        });
    }

    /// Read-modify-write
    ///
    /// Always reads the most recent write
    pub fn rmw<F>(&self, f: F, order: Ordering) -> T
    where
        F: FnOnce(T) -> T,
    {
        rt::branch();
        let mut writes = self.writes.borrow_mut();

        synchronize(|execution| {
            let old = {
                let write = writes.last_mut().unwrap();
                write.first_seen.touch(&execution.threads.actor());
                write.sync.sync_read(execution, order);
                write.value
            };

            do_write(f(old), &mut *writes, execution, order);
            old
        })
    }

    pub fn swap(&self, val: T, order: Ordering) -> T {
        self.rmw(|_| val, order)
    }

    pub fn compare_and_swap(&self, current: T, new: T, order: Ordering) -> T {
        use self::Ordering::*;

        let failure = match order {
            Relaxed | Release => Relaxed,
            Acquire | AcqRel => Acquire,
            _ => SeqCst,
        };

        match self.compare_exchange(current, new, order, failure) {
            Ok(v) => v,
            Err(v) => v,
        }
    }

    pub fn compare_exchange(
        &self,
        current: T,
        new: T,
        success: Ordering,
        failure: Ordering
    ) -> Result<T, T>
    {
        rt::branch();
        let mut writes = self.writes.borrow_mut();

        synchronize(|execution| {
            {
                let write = writes.last_mut().unwrap();
                write.first_seen.touch(&execution.threads.actor());

                if write.value != current {
                    write.sync.sync_read(execution, failure);
                    return Err(write.value);
                }

                write.sync.sync_read(execution, success);
            }

            do_write(new, &mut *writes, execution, success);
            Ok(current)
        })
    }
}

fn pick_write<'a, T>(
    writes: &'a mut [Write<T>],
    execution: &mut Execution,
    order: Ordering,
) -> &'a mut Write<T>
{
    let mut in_causality = false;
    let threads = &mut execution.threads;

    let next = execution.path.branch_write({
        writes.iter()
            .enumerate()
            .rev()
            // Explore all writes that are not within the actor's causality as
            // well as the latest one.
            .take_while(|&(_, ref write)| {
                let ret = in_causality;

                in_causality |= is_seq_cst(order) && write.seq_cst;
                in_causality |= write.first_seen.is_seen_by(&threads.actor());

                !ret
            })
            .map(|(i, _)| i)
    });

    &mut writes[next]
}

fn do_write<T>(
    value: T,
    writes: &mut Vec<Write<T>>,
    execution: &mut Execution,
    order: Ordering)
{
    let mut write = Write {
        value,
        sync: writes.last().unwrap().sync.clone_with(&mut execution.arena),
        first_seen: FirstSeen::new(execution),
        seq_cst: is_seq_cst(order),
    };

    write.sync.sync_write(execution, order);
    writes.push(write);
}

fn synchronize<F, R>(f: F) -> R
where
    F: FnOnce(&mut Execution) -> R
{
    rt::execution(|execution| {
        let ret = f(execution);
        execution.threads.actor().inc();
        ret
    })
}

fn is_seq_cst(order: Ordering) -> bool {
    match order {
        Ordering::SeqCst => true,
        _ => false,
    }
}

impl FirstSeen {
    fn new(execution: &mut Execution) -> FirstSeen {
        let mut first_seen = FirstSeen(vec![]);
        first_seen.touch(&execution.threads.actor());

        first_seen
    }

    fn touch(&mut self, actor: &Actor) {
        let happens_before = actor.happens_before();

        if self.0.len() < happens_before.len() {
            self.0.resize(happens_before.len(), None);
        }

        if self.0[actor.id()].is_none() {
            self.0[actor.id()] = Some(actor.self_version());
        }
    }

    fn is_seen_by(&self, actor: &Actor) -> bool {
        for (thread_id, version) in actor.happens_before().versions() {
            let seen = self.0.get(thread_id)
                .and_then(|maybe_version| *maybe_version)
                .map(|v| v <= version)
                .unwrap_or(false);

            if seen {
                return true;
            }
        }

        false
    }
}
