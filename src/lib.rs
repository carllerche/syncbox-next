#[macro_use]
extern crate cfg_if;

cfg_if! {
    if #[cfg(feature = "futures")] {
        extern crate futures as _futures;
        pub mod futures;
    }
}

#[cfg(feature = "fuzz")]
extern crate syncbox_fuzz;
