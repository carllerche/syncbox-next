#[macro_use]
extern crate cfg_if;
extern crate crossbeam_utils;

mod causal;
mod parker;

pub use self::causal::CausalCell;
pub use self::parker::Parker;

cfg_if! {
    if #[cfg(feature = "futures")] {
        extern crate futures as _futures;
        pub mod futures;
    }
}

#[cfg(feature = "fuzz")]
extern crate syncbox_fuzz;
