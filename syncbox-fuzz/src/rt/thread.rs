use fringe::{
    Generator,
    OsStack,
    generator::Yielder,
};

use std::cell::Cell;
use std::ptr;

#[derive(Debug)]
pub struct Thread {
    generator: Option<Generator<'static, (), (), OsStack>>,
}

thread_local!(static YIELDER: Cell<*const Yielder<(), ()>> = Cell::new(ptr::null()));

impl Thread {
    pub fn new<F: FnOnce() + 'static>(stack: OsStack, body: F) -> Thread {
        let generator = Generator::new(stack, move |yielder, ()| {
            struct UnsetTls;

            impl Drop for UnsetTls {
                fn drop(&mut self) {
                    YIELDER.with(|cell| cell.set(ptr::null()));
                }
            }

            let _reset = UnsetTls;

            let ptr = yielder as *const _;
            YIELDER.with(|cell| cell.set(ptr));

            body();
        });

        Thread {
            generator: Some(generator),
        }
    }

    pub fn suspend() {
        let ptr = YIELDER.with(|cell| {
            let ptr = cell.get();
            cell.set(ptr::null());
            ptr
        });

        unsafe { ptr.as_ref().unwrap().suspend(()); }

        YIELDER.with(|cell| cell.set(ptr));
    }

    pub fn resume(&mut self) -> Option<OsStack> {
        {
            let generator = self.generator
                .as_mut()
                .unwrap();

            if generator.resume(()).is_some() {
                return None;
            }
        }

        let stack = self.generator.take().unwrap().unwrap();
        Some(stack)
    }
}
