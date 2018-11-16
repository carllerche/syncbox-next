#[macro_use]
extern crate cfg_if;
extern crate crossbeam_utils;

mod causal;

pub use self::causal::CausalCell;

cfg_if! {
    if #[cfg(feature = "futures")] {
        extern crate futures as _futures;
        pub mod futures;
    }
}

#[cfg(feature = "fuzz")]
extern crate syncbox_fuzz;
