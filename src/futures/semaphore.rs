cfg_if! {
    if #[cfg(fuzz)] {
        use syncbox_fuzz::{
            futures::AtomicTask,
            sync::{
                CausalCell,
                atomic::{AtomicUsize, AtomicPtr},
            },
        };
    } else {
        use CausalCell;
        use _futures::task::AtomicTask;
        use std::sync::atomic::{AtomicUsize, AtomicPtr};
    }
}

use crossbeam_utils::CachePadded;
use _futures::Poll;

use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::sync::atomic::Ordering::{self, Acquire, Release, AcqRel, Relaxed};

/// Futures-aware semaphore.
pub struct Semaphore {
    /// Tracks both the waiter queue tail pointer and the number of remaining
    /// permits.
    state: CachePadded<AtomicUsize>,

    /// waiter queue head pointer.
    head: CausalCell<NonNull<WaiterNode>>,

    /// Coordinates access to the queue head.
    rx_lock: AtomicUsize,

    /// Stub waiter node used as part of the MPSC channel algorithm.
    stub: Box<WaiterNode>,
}

/// Wait on a semaphore.
#[derive(Debug)]
pub struct Waiter(Option<Arc<WaiterNode>>);

/// Node used to notify the semaphore waiter when permit is available.
#[derive(Debug)]
struct WaiterNode {
    /// Stores waiter state.
    ///
    /// See `NodeState` for more details.
    state: AtomicUsize,

    /// Task to notify when a permit is made available.
    task: AtomicTask,

    /// Next pointer in the queue of waiting senders.
    next: AtomicPtr<WaiterNode>,
}

/// Semaphore state
///
/// The 2 low bits track the modes.
///
/// - Closed
/// - Full
///
/// When not full, the rest of the `usize` tracks the total number of messages
/// in the channel. When full, the rest of the `usize` is a pointer to the tail
/// of the "waiting senders" queue.
#[derive(Debug, Copy, Clone)]
struct SemState(usize);

/// Waiter node state
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(usize)]
enum NodeState {
    /// Not waiting for a permit and the node is not in the wait queue.
    ///
    /// This is the initial state.
    Idle = 0,

    /// Not waiting for a permit but the node is in the wait queue.
    ///
    /// This happens when the waiter has previously requested a permit, but has
    /// since canceled the request. The node cannot be removed by the waiter, so
    /// this state informs the receiver to skip the node when it pops it from
    /// the wait queue.
    Queued = 1,

    /// Waiting for a permit and the node is in the wait queue.
    QueuedWaiting = 2,

    /// The waiter has been assigned a permit and the node has been removed from
    /// the queue.
    Assigned = 3,
}

// ===== impl Semaphore =====

impl Semaphore {
    /// Creates a new semaphore with the initial number of permits
    ///
    /// # Panics
    ///
    /// Panics if `permits` is zero.
    pub fn new(permits: usize) -> Semaphore {
        assert!(permits > 0, "permits must be greater than zero");

        let stub = Box::new(WaiterNode::new());
        let ptr = NonNull::new(&*stub as *const _ as *mut _).unwrap();

        // Allocations are aligned
        debug_assert!(ptr.as_ptr() as usize & NUM_FLAG == 0);

        let state = SemState::new(permits);

        Semaphore {
            state: CachePadded::new(AtomicUsize::new(state.to_usize())),
            head: CausalCell::new(ptr),
            rx_lock: AtomicUsize::new(0),
            stub,
        }
    }

    /// Returns the current number of available permits
    pub fn available_permits(&self) -> usize {
        let curr = SemState::load(&self.state, Acquire);
        curr.available_permits()
    }

    /// Poll for a permit
    pub fn poll_permit(&self, mut waiter: Option<&mut Waiter>) -> Poll<(), ()> {
        use _futures::Async::*;

        // Load the current state
        let mut curr = SemState::load(&self.state, Acquire);

        // Tracks a *mut WaiterNode representing an Arc clone.
        //
        // This avoids having to bump the ref count unless required.
        let mut maybe_strong = None;

        loop {
            let mut next = curr;

            if !next.acquire_permit(&self.stub) {
                debug_assert!(curr.waiter().is_some());

                if maybe_strong.is_none() {
                    if let Some(ref mut waiter) = waiter {
                        // Get the Sender's waiter node, or initialize one
                        let waiter = waiter.0
                            .get_or_insert_with(|| Arc::new(WaiterNode::new()));

                        waiter.register();

                        if !waiter.to_queued_waiting() {
                            // The node is alrady queued, there is no further work
                            // to do.
                            return Ok(NotReady);
                        }

                        maybe_strong = Some(WaiterNode::into_non_null(waiter.clone()));
                    } else {
                        // If no `waiter`, then the task is not registered and there
                        // is no further work to do.
                        return Err(());
                    }
                }

                next.set_waiter(maybe_strong.unwrap());
            }

            debug_assert_ne!(curr.0, 0);
            debug_assert_ne!(next.0, 0);

            match next.compare_exchange(&self.state, curr, AcqRel, Acquire) {
                Ok(_) => {
                    match curr.waiter() {
                        Some(prev_waiter) => {
                            let waiter = maybe_strong.unwrap();

                            // Finish pushing
                            unsafe {
                                prev_waiter.as_ref()
                                    .next.store(waiter.as_ptr(), Release);
                            }

                            return Ok(NotReady);
                        }
                        None => {
                            if let Some(waiter) = maybe_strong {
                                // The waiter was cloned, but never got queued.
                                // Before enterig `inc_num_messages`, the waiter was
                                // in the `Idle` state. We must transition the node
                                // back to the idle state.
                                let waiter = unsafe { Arc::from_raw(waiter.as_ptr()) };
                                waiter.revert_to_idle();
                            }

                            return Ok(Ready(()));
                        }
                    }
                }
                Err(actual) => {
                    curr = actual;
                }
            }
        }
    }

    /// Release one permit back to the sempahore.
    ///
    /// This either increments the number of available permits or notifies a
    /// pending waiter.
    pub fn release_one(&self) {
        let prev = self.rx_lock.fetch_add(1, AcqRel);

        if prev != 0 {
            // Another thread has the lock and will be responsible for notifying
            // pending waiters.
            return;
        }

        // The remaining amount of permits to release back to the semaphore.
        let mut rem = 1;

        while rem > 0 {
            // Release the permits
            self.release_n(rem);

            let actual = self.rx_lock.fetch_sub(rem, AcqRel);

            rem = actual - rem;
        }
    }

    /// Release a specific amount of permits to the semaphore
    pub fn release_n(&self, mut n: usize) {
        while n > 0 {
            let waiter = match self.pop(n) {
                Some(waiter) => waiter,
                None => {
                    return;
                }
            };

            if waiter.notify() {
                n -= 1;
            }
        }
    }

    /// Pop a waiter
    ///
    /// `rem` represents the remaining number of times the caller will pop. If
    /// there are no more waiters to pop, `rem` is used to set the available
    /// permits.
    fn pop(&self, rem: usize) -> Option<Arc<WaiterNode>> {
        'outer:
        loop {
            unsafe {
                let mut head = self.head.with(|head| *head);
                let mut next_ptr = head.as_ref().next.load(Acquire);

                let stub = self.stub();

                if head == stub {
                    let next = match NonNull::new(next_ptr) {
                        Some(next) => next,
                        None => {
                            // This loop is not part of the standard intrusive mpsc
                            // channel algorithm. This is where we atomically pop
                            // the last task and add `rem` to the remaining capacity.
                            //
                            // This modification to the pop algorithm works because,
                            // at this point, we have not done any work (only done
                            // reading). We have a *pretty* good idea that there is
                            // no concurrent pusher.
                            //
                            // The capacity is then atomically added by doing an
                            // AcqRel CAS on `state`. The `state` cell is the
                            // linchpin of the algorithm.
                            //
                            // By successfully CASing `head` w/ AcqRel, we ensure
                            // that, if any thread was racing and entered a push, we
                            // see that and abort pop, retrying as it is
                            // "inconsistent".
                            let mut curr = SemState::load(&self.state, Acquire);

                            loop {
                                if curr.has_waiter(&self.stub) {
                                    // Inconsistent
                                    continue 'outer;
                                }

                                let mut next = curr;
                                next.release_permits(rem, &self.stub);

                                match next.compare_exchange(&self.state, curr, AcqRel, Acquire) {
                                    Ok(_) => return None,
                                    Err(actual) => {
                                        curr = actual;
                                    }
                                }
                            }
                        }
                    };

                    self.head.with_mut(|head| *head = next);
                    head = next;
                    next_ptr = next.as_ref().next.load(Acquire);
                }

                if let Some(next) = NonNull::new(next_ptr) {
                    self.head.with_mut(|head| *head = next);

                    return Some(Arc::from_raw(head.as_ptr()));
                }

                let state = SemState::load(&self.state, Acquire);

                // This must always be a pointer as the wait list is not empty.
                let tail = state.waiter().unwrap();

                if tail != head {
                    // Inconsistent
                    continue 'outer;
                }

                self.push_stub();

                next_ptr = head.as_ref().next.load(Acquire);

                if let Some(next) = NonNull::new(next_ptr) {
                    self.head.with_mut(|head| *head = next);

                    return Some(Arc::from_raw(head.as_ptr()));
                }

                // Inconsistent state, loop
            }
        }
    }

    unsafe fn push_stub(&self) {
        let stub = self.stub();

        // Set the next pointer. This does not require an atomic operation as
        // this node is not accessible. The write will be flushed with the next
        // operation
        stub.as_ref().next.store(ptr::null_mut(), Relaxed);


        // Update the tail to point to the new node. We need to see the previous
        // node in order to update the next pointer as well as release `task`
        // to any other threads calling `push`.
        let prev = SemState::new_ptr(stub)
            .swap(&self.state, AcqRel);

        // The stub is only pushed when there are pending tasks. Because of
        // this, the state must *always* be in pointer mode.
        let prev = prev.waiter().unwrap();

        // We don't want the *existing* pointer to be a stub.
        debug_assert_ne!(prev, stub);

        // Release `task` to the consume end.
        prev.as_ref().next.store(stub.as_ptr(), Release);
    }

    fn stub(&self) -> NonNull<WaiterNode> {
        unsafe {
            NonNull::new_unchecked(&*self.stub as *const _ as *mut _)
        }
    }
}

// ===== impl Waiter =====

impl Waiter {
    pub fn new() -> Waiter {
        Waiter(None)
    }

    pub fn acquire(&self) -> bool {
        self.0.as_ref()
            .map(|node| node.acquire())
            .unwrap_or(false)
    }
}

// ===== impl WaiterNode =====

impl WaiterNode {
    fn new() -> WaiterNode {
        WaiterNode {
            state: AtomicUsize::new(NodeState::new().to_usize()),
            task: AtomicTask::new(),
            next: AtomicPtr::new(ptr::null_mut()),
        }
    }

    fn acquire(&self) -> bool {
        use self::NodeState::*;
        Idle.compare_exchange(&self.state, Assigned, AcqRel, Acquire).is_ok()
    }

    fn register(&self) {
        self.task.register()
    }

    /// Transition the state to `QueuedWaiting`.
    ///
    /// This step can only happen from `Queued` or from `Idle`.
    ///
    /// Returns `true` if transitioning into a queued state.
    fn to_queued_waiting(&self) -> bool {
        use self::NodeState::*;

        let mut curr = NodeState::load(&self.state, Acquire);

        loop {
            debug_assert!(curr == Idle || curr == Queued, "actual = {:?}", curr);
            let next = QueuedWaiting;

            match next.compare_exchange(&self.state, curr, AcqRel, Acquire) {
                Ok(_) => {
                    if curr.is_queued() {
                        return false;
                    } else {
                        // Transitioned to queued, reset next pointer
                        self.next.store(ptr::null_mut(), Relaxed);
                        return true;
                    }
                }
                Err(actual) => {
                    curr = actual;
                }
            }
        }
    }

    /// Notify the waiter
    ///
    /// Returns `true` if the waiter accepts the notification
    fn notify(&self) -> bool {
        use self::NodeState::*;

        // Assume QueuedWaiting state
        let mut curr = QueuedWaiting;

        loop {
            let next = match curr {
                Queued => Idle,
                QueuedWaiting => Assigned,
                actual => panic!("actual = {:?}", actual),
            };

            match next.compare_exchange(&self.state, curr, AcqRel, Acquire) {
                Ok(_) => {
                    match curr {
                        QueuedWaiting => {
                            self.task.notify();
                            return true;
                        }
                        _ => {
                            return false;
                        }
                    }
                }
                Err(actual) => {
                    curr = actual
                }
            }
        }
    }

    fn revert_to_idle(&self) {
        use self::NodeState::Idle;

        // There are no other handles to the node
        NodeState::store(&self.state, Idle, Relaxed);
    }

    fn into_non_null(arc: Arc<WaiterNode>) -> NonNull<WaiterNode> {
        let ptr = Arc::into_raw(arc);
        unsafe { NonNull::new_unchecked(ptr as *mut _) }
    }
}

// ===== impl State =====

/// Flag differentiating between available permits and waiter pointers.
///
/// If we assume pointers are properly aligned, then the least significant bit
/// will always be zero. So, we use that bit to track if the value represents a
/// number.
const NUM_FLAG: usize = 0b1;

/// When representing "numbers", the state has to be shifted this much (to get
/// rid of the flag bit).
const NUM_SHIFT: usize = 1;

impl SemState {
    /// Returns a new default `State` value.
    fn new(permits: usize) -> SemState {
        SemState((permits << NUM_SHIFT) | NUM_FLAG)
    }

    /// Returns a `State` tracking `ptr` as the tail of the queue.
    fn new_ptr(tail: NonNull<WaiterNode>) -> SemState {
        SemState(tail.as_ptr() as usize)
    }

    /// Returns the amount of remaining capacity
    fn available_permits(&self) -> usize {
        if !self.has_available_permits() {
            return 0;
        }

        self.0 >> NUM_SHIFT
    }

    /// Returns true if the state has permits that can be claimed by a waiter.
    fn has_available_permits(&self) -> bool {
        self.0 & NUM_FLAG == NUM_FLAG
    }

    fn has_waiter(&self, stub: &WaiterNode) -> bool {
        !self.has_available_permits() && !self.is_stub(stub)
    }

    /// Try to acquire a permit
    ///
    /// # Return
    ///
    /// Returns `true` if the permit was acquired, `false` otherwise. If `false`
    /// is returned, it can be assumed that `State` represents the head pointer
    /// in the mpsc channel.
    fn acquire_permit(&mut self, stub: &WaiterNode) -> bool {
        if !self.has_available_permits() {
            return false;
        }

        debug_assert!(self.0 != 1);
        debug_assert!(self.waiter().is_none());

        self.0 -= 1 << NUM_SHIFT;

        if self.0 == NUM_FLAG {
            // Set the state to the stub pointer.
            self.0 = stub as *const _ as usize;
        }

        true
    }

    /// Release permits
    ///
    /// Returns `true` if the permits were accepted.
    fn release_permits(&mut self, permits: usize, stub: &WaiterNode) {
        debug_assert!(permits > 0);

        if self.is_stub(stub) {
            self.0 = (permits << NUM_SHIFT) | NUM_FLAG;
            return;
        }

        debug_assert!(self.has_available_permits());

        self.0 += permits << NUM_SHIFT;
    }

    fn is_waiter(&self) -> bool {
        self.0 & NUM_FLAG == 0
    }

    /// Returns the waiter, if one is set.
    fn waiter(&self) -> Option<NonNull<WaiterNode>> {
        if self.is_waiter() {
            let waiter = NonNull::new(self.0 as *mut WaiterNode)
                .expect("null pointer stored");

            Some(waiter)
        } else {
            None
        }
    }

    /// Set to a pointer to a waiter.
    ///
    /// This can only be done from the full state.
    fn set_waiter(&mut self, waiter: NonNull<WaiterNode>) {
        let waiter = waiter.as_ptr() as usize;
        debug_assert!(waiter & NUM_FLAG == 0);

        self.0 = waiter;
    }

    fn is_stub(&self, stub: &WaiterNode) -> bool {
        self.0 == stub as *const _ as usize
    }

    /// Load the state from an AtomicUsize.
    fn load(cell: &AtomicUsize, ordering: Ordering) -> SemState {
        SemState(cell.load(ordering))
    }

    /// Swap the values
    fn swap(&self, cell: &AtomicUsize, ordering: Ordering) -> SemState {
        SemState(cell.swap(self.to_usize(), ordering))
    }

    /// Compare and exchange the current value into the provided cell
    fn compare_exchange(&self,
                        cell: &AtomicUsize,
                        prev: SemState,
                        success: Ordering,
                        failure: Ordering)
        -> Result<SemState, SemState>
    {
        cell.compare_exchange(prev.to_usize(), self.to_usize(), success, failure)
            .map(SemState)
            .map_err(SemState)
    }

    /// Converts the state into a `usize` representation.
    fn to_usize(&self) -> usize {
        self.0
    }
}

// ===== impl NodeState =====

impl NodeState {
    fn new() -> NodeState {
        NodeState::Idle
    }

    fn from_usize(value: usize) -> NodeState {
        use self::NodeState::*;

        match value {
            0 => Idle,
            1 => Queued,
            2 => QueuedWaiting,
            3 => Assigned,
            _ => panic!(),
        }
    }

    fn load(cell: &AtomicUsize, ordering: Ordering) -> NodeState {
        NodeState::from_usize(cell.load(ordering))
    }

    /// Store a value
    fn store(cell: &AtomicUsize, value: NodeState, ordering: Ordering) {
        cell.store(value.to_usize(), ordering);
    }

    fn compare_exchange(&self,
                        cell: &AtomicUsize,
                        prev: NodeState,
                        success: Ordering,
                        failure: Ordering)
        -> Result<NodeState, NodeState>
    {
        cell.compare_exchange(prev.to_usize(), self.to_usize(), success, failure)
            .map(NodeState::from_usize)
            .map_err(NodeState::from_usize)
    }

    /// Returns `true` if `self` represents a queued state.
    pub fn is_queued(&self) -> bool {
        use self::NodeState::*;

        match *self {
            Queued | QueuedWaiting => true,
            _ => false,
        }
    }

    fn to_usize(&self) -> usize {
        *self as usize
    }
}
