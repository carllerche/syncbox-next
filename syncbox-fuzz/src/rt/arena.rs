use std::alloc::Layout;
use std::cell::Cell;
use std::cmp;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::rc::Rc;
use std::slice;

#[derive(Debug)]
pub struct Arena {
    inner: Rc<Inner>,
}

pub struct Slice<T> {
    ptr: *mut T,
    len: usize,
    _inner: Rc<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// Head of the arena space
    head: *mut u8,

    /// Offset into the last region
    pos: Cell<usize>,

    /// Total capacity of the arena
    cap: usize,
}

impl Arena {
    /// Create an `Arena` with specified capacity.
    ///
    /// Capacity must be a power of 2. The capacity cannot be grown after the fact.
    pub fn with_capacity(capacity: usize) -> Arena {
        let head = unsafe {
            libc::mmap(
                ptr::null_mut(),
                capacity,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANON | libc::MAP_PRIVATE,
                -1,
                0,
            )
        };

        Arena {
            inner: Rc::new(Inner {
                head: head as *mut u8,
                pos: Cell::new(0),
                cap: capacity,
            }),
        }
    }

    pub fn slice<T>(&mut self, len: usize) -> Slice<T>
    where
        T: Default,
    {
        let ptr: *mut T = self.allocate(len);

        for i in 0..len {
            unsafe {
                ptr::write(ptr.offset(i as isize), T::default());
            }
        }

        Slice {
            ptr,
            len,
            _inner: self.inner.clone(),
        }
    }

    pub fn clear(&mut self) {
        assert!(1 == Rc::strong_count(&self.inner));
        self.inner.pos.set(0);
    }

    fn allocate<T>(&mut self, count: usize) -> *mut T {
        let layout = Layout::new::<T>();
        let mask = layout.align() - 1;
        let pos = self.inner.pos.get();

        debug_assert!(layout.align() >= (pos & mask));

        let mut skip = layout.align() - (pos & mask);

        if skip == layout.align() {
            skip = 0;
        }

        let additional = skip + layout.size() * count;

        assert!(pos + additional <= self.inner.cap, "arena overflow");

        self.inner.pos.set(pos + additional);

        let ret = unsafe { self.inner.head.offset((pos + skip) as isize) as *mut T };

        debug_assert!((ret as usize) >= self.inner.head as usize);
        debug_assert!((ret as usize) < (self.inner.head as usize + self.inner.cap));

        ret
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let res = unsafe { libc::munmap(self.head as *mut libc::c_void, self.cap) };

        // TODO: Do something on error
        debug_assert_eq!(res, 0);
    }
}

impl<T: Clone> Slice<T> {
    pub fn clone_with(&self, arena: &mut Arena) -> Slice<T> {
        let ptr: *mut T = arena.allocate(self.len);

        for i in 0..self.len {
            unsafe {
                ptr::write(ptr.offset(i as isize), self[i].clone());
            }
        }

        Slice {
            ptr,
            len: self.len,
            _inner: arena.inner.clone(),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Slice<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.deref().fmt(fmt)
    }
}

impl<T> Deref for Slice<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl<T> DerefMut for Slice<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl<T: Eq> Eq for Slice<T> {}

impl<T: PartialEq> PartialEq for Slice<T> {
    fn eq(&self, other: &Self) -> bool {
        self.deref().eq(other.deref())
    }
}

impl<T: PartialOrd> PartialOrd for Slice<T> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.deref().partial_cmp(other.deref())
    }
}

impl<T> Drop for Slice<T> {
    fn drop(&mut self) {
        for i in 0..self.len {
            unsafe {
                ptr::read(self.ptr.offset(i as isize) as *const _);
            }
        }
    }
}
