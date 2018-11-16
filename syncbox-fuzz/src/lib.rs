#[macro_use]
extern crate cfg_if;
extern crate fringe;
#[macro_use]
extern crate scoped_mut_tls;

macro_rules! if_futures {
    ($($t:tt)*) => {
        cfg_if! {
            if #[cfg(feature = "futures")] {
                $($t)*
            }
        }
    }
}

mod check;
mod rt;
pub mod sync;
pub mod thread;

pub use check::fuzz;

if_futures! {
    extern crate futures as _futures;

    pub mod futures;

    pub use check::fuzz_future;
}
