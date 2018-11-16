use std::cell::UnsafeCell;

pub struct CausalCell<T>(UnsafeCell<T>);

impl<T> CausalCell<T> {
    pub fn new(data: T) -> CausalCell<T> {
        CausalCell(UnsafeCell::new(data))
    }

    pub unsafe fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        f(&*self.0.get())
    }

    pub unsafe fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        f(&mut *self.0.get())
    }
}
