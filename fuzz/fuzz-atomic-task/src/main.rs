#[macro_use]
extern crate cfg_if;
extern crate futures as _futures;
extern crate syncbox_fuzz;

#[path = "../../../src/futures/atomic_task.rs"]
mod atomic_task;

pub use atomic_task::AtomicTask;

fn main() {
    println!("Hello, world!");
}
