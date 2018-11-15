cfg_if! {
    if #[cfg(fuzz)] {
        use syncbox_fuzz::{
            sync::{
                CausalCell,
                atomic::AtomicUsize,
            },
        };
    } else {

        use std::cell::UnsafeCell;
        use std::sync::atomic::AtomicUsize;
    }
}

use _futures::task::{self, Task};

use std::fmt;
use std::sync::atomic::Ordering::{Acquire, Release, AcqRel, SeqCst};

/// TODO: Dox
pub struct AtomicTask {
    state: AtomicUsize,
    task: CausalCell<Option<Task>>,
}

const EMPTY: usize = 0;

const HOLDS: usize = 0b100;

const REGISTERING: usize = 0b001;

const WAKING: usize = 0b010;


impl AtomicTask {
    /// TODO: Dox
    pub fn new() -> AtomicTask {
        // Make sure that task is Sync
        trait AssertSync: Sync {}
        impl AssertSync for Task {}

        AtomicTask {
            state: AtomicUsize::new(EMPTY),
            task: CausalCell::new(None),
        }
    }

    pub fn register(&self) {
        self.register_task(task::current());
    }

    /// TODO: Dox
    pub fn register_task(&self, task: Task) {
        let state = self.state.load(SeqCst);
        if (state == EMPTY || state == HOLDS)
            && self.state.compare_and_swap(state, REGISTERING, Acquire) == state
        {
            return unsafe {
                debug_assert_eq!(state == HOLDS, self.task.with(|t| t.is_some()));

                // Locked acquired, update the task cell
                self.task.with_mut(|t| *t = Some(task));

                // Release the lock. If the state transitioned to include
                // the `WAKING` bit, this means that a notify has been
                // called concurrently, so we have to remove the task and
                // notify it.`
                //
                // Start by assuming that the state is `REGISTERING` as this
                // is what we jut set it to.
                let res = self.state.compare_exchange(
                    REGISTERING, HOLDS, AcqRel, Acquire);

                match res {
                    Ok(_) => {}
                    Err(actual) => {
                        // This branch can only be reached if a
                        // concurrent thread called `notify`. In this
                        // case, `actual` **must** be `REGISTERING |
                        // `WAKING`.
                        debug_assert_eq!(actual, REGISTERING | WAKING);

                        // Take the task to notify once the atomic operation has
                        // completed.
                        let task = self.task.with_mut(|t| t.take()).unwrap();

                        // Just swap, because no one could change state while state == `REGISTERING` | `WAKING`.
                        self.state.swap(EMPTY, AcqRel);

                        // The atomic swap was complete, now
                        // notify the task and return.
                        task.notify();
                    }
                }
            }
        }

        if state == WAKING || state == WAKING | HOLDS || state == EMPTY || state == HOLDS {
            // Currently in the process of waking the task, i.e.,
            // `wake` is currently being called on the old task handle.
            // So, we call wake on the new task
            task.notify();
        } else {
            // In this case, a concurrent thread is holding the
            // "registering" lock. This probably indicates a bug in the
            // caller's code as racing to call `register` doesn't make much
            // sense.
            //
            // We just want to maintain memory safety. It is ok to drop the
            // call to `register`.
            debug_assert!(
                state == REGISTERING ||
                state == REGISTERING | WAKING);
        }
    }

    /// TODO: Dox
    pub fn notify(&self) {
        let state = self.state.load(SeqCst);

        if state == EMPTY || state & WAKING != 0 {
            // One of:
            // * no task inside, nothing to wake
            // * another process is calling wake now
            return;
        }

        // AcqRel ordering is used in order to acquire the value of the `task`
        // cell as well as to establish a `release` ordering with whatever
        // memory the `AtomicTask` is associated with.
        let state = self.state.fetch_or(WAKING, AcqRel);
        match state {
            EMPTY | HOLDS => {
                // The waking lock has been acquired.
                let task = unsafe { self.task.with_mut(|t| t.take()) };
                debug_assert_eq!(state == HOLDS, task.is_some());

                // Release the lock
                self.state.fetch_and(!(WAKING | HOLDS), Release);

                if let Some(task) = task {
                    task.notify();
                }
            }
            state => {
                // There is a concurrent thread currently updating the
                // associated task.
                //
                // Nothing more to do as the `WAKING` bit has been set. It
                // doesn't matter if there are concurrent registering threads or
                // not.
                //
                debug_assert!(
                    state == REGISTERING ||
                    state == REGISTERING | WAKING ||
                    state == WAKING ||
                    state == WAKING | HOLDS);
            }
        }
    }
}

impl Default for AtomicTask {
    fn default() -> Self {
        AtomicTask::new()
    }
}

impl fmt::Debug for AtomicTask {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "AtomicTask")
    }
}

unsafe impl Send for AtomicTask {}
unsafe impl Sync for AtomicTask {}
