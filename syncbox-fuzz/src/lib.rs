extern crate fringe;
#[macro_use]
extern crate scoped_mut_tls;

#[cfg(feature = "futures")]
extern crate futures as _futures;

mod check;
mod rt;
pub mod sync;
pub mod thread;

#[cfg(feature = "futures")]
pub mod futures;

pub use check::check;

#[cfg(feature = "futures")]
pub use check::check_future;
