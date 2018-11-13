extern crate fringe;
#[macro_use]
extern crate scoped_mut_tls;

mod check;
mod rt;
pub mod sync;
pub mod thread;

pub use check::check;
