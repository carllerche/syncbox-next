#[macro_use]
extern crate cfg_if;
extern crate generator;
extern crate libc;
#[macro_use]
extern crate scoped_tls;
#[macro_use]
extern crate scoped_mut_tls;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

macro_rules! if_futures {
    ($($t:tt)*) => {
        cfg_if! {
            if #[cfg(feature = "futures")] {
                $($t)*
            }
        }
    }
}

#[macro_export]
macro_rules! debug {
    ($($t:tt)*) => {
        if $crate::__debug_enabled() {
            println!($($t)*);
        }
    };
}

pub mod fuzz;
mod rt;
pub mod sync;
pub mod thread;

pub use fuzz::fuzz;

if_futures! {
    extern crate futures as _futures;

    pub mod futures;

    pub use fuzz::fuzz_future;
}

pub use rt::yield_now;

#[doc(hidden)]
pub fn __debug_enabled() -> bool {
    rt::Scheduler::with_execution(|e| e.log)
}
