use rt::{self, Actor, CausalContext, Synchronize};

use std::cell::RefCell;
use std::sync::atomic::Ordering;

/// An atomic value
pub struct Atomic<T> {
    writes: RefCell<Vec<Write<T>>>,
}

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

struct FirstSeen(Vec<Option<usize>>);

impl<T> Atomic<T>
where
    T: Copy + PartialEq,
{
    pub fn new(value: T) -> Atomic<T> {
        rt::causal_context(|ctx| {
            let writes = vec![Write {
                value,
                sync: Synchronize::new(),
                first_seen: FirstSeen::new(ctx),
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

        synchronize(|ctx| {
            // Pick a write that satisfies causality and specified ordering.
            let write = pick_write(&mut writes[..], ctx, order);
            write.first_seen.touch(ctx.actor());
            write.sync.sync_read(ctx, order);
            write.value
        })
    }

    pub fn store(&self, val: T, order: Ordering) {
        rt::branch();
        let mut writes = self.writes.borrow_mut();

        synchronize(|ctx| {
            do_write(val, &mut *writes, ctx, order);
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

        synchronize(|ctx| {
            let old = {
                let write = writes.last_mut().unwrap();
                write.first_seen.touch(ctx.actor());
                write.sync.sync_read(ctx, order);
                write.value
            };

            do_write(f(old), &mut *writes, ctx, order);
            old
        })
    }

    pub fn swap(&self, val: T, order: Ordering) -> T {
        self.rmw(|_| val, order)
    }

    pub fn compare_and_swap(&self, current: T, new: T, order: Ordering) -> T {
        match self.compare_exchange(current, new, order, order) {
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

        synchronize(|ctx| {
            {
                let write = writes.last_mut().unwrap();
                write.first_seen.touch(ctx.actor());

                if write.value != current {
                    write.sync.sync_read(ctx, failure);
                    return Err(write.value);
                }

                write.sync.sync_read(ctx, success);
            }

            do_write(new, &mut *writes, ctx, success);
            Ok(current)
        })
    }
}

fn pick_write<'a, T>(
    writes: &'a mut [Write<T>],
    ctx: &mut CausalContext,
    order: Ordering,
) -> &'a mut Write<T>
{
    let seq_cst = is_seq_cst(order);
    let lower_bound = newest_in_causality(writes, ctx, seq_cst);

    let offset = ctx.seed.pop_front()
        .map(|branch| {
            assert!(!branch.switch);
            branch.index
        })
        .unwrap_or(0);

    let last = writes.len() - 1 == lower_bound + offset;

    ctx.branches.push(rt::Branch {
        switch: false,
        index: offset,
        last,
    });

    &mut writes[lower_bound + offset]
}

fn do_write<T>(
    value: T,
    writes: &mut Vec<Write<T>>,
    ctx: &mut CausalContext,
    order: Ordering)
{
    let mut write = Write {
        value,
        sync: writes.last().unwrap().sync.clone(),
        first_seen: FirstSeen::new(ctx),
        seq_cst: is_seq_cst(order),
    };

    write.sync.sync_write(ctx, order);
    writes.push(write);
}

/// Find the newest write that is contained in the current actor's causality.
///
/// The atomic load may not return an older write.
fn newest_in_causality<T>(
    writes: &[Write<T>],
    ctx: &mut CausalContext,
    seq_cst: bool,
) -> usize
{
    for (i, write) in writes.iter().enumerate().rev() {
        if seq_cst && write.seq_cst {
            return i;
        }

        if write.first_seen.is_seen_by(ctx.actor()) {
            return i;
        }
    }

    0
}

fn synchronize<F, R>(f: F) -> R
where
    F: FnOnce(&mut CausalContext) -> R
{
    rt::causal_context(|ctx| {
        let ret = f(ctx);
        ctx.actor().inc();
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
    fn new(ctx: &mut CausalContext) -> FirstSeen {
        let mut first_seen = FirstSeen(vec![]);
        first_seen.touch(ctx.actor());

        first_seen
    }

    fn touch(&mut self, actor: &Actor) {
        let causality = actor.causality();

        if self.0.len() < causality.len() {
            self.0.resize(causality.len(), None);
        }

        self.0[actor.id()] = Some(actor.self_version());
    }

    fn is_seen_by(&self, actor: &Actor) -> bool {
        for (thread_id, version) in actor.causality().versions() {
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
