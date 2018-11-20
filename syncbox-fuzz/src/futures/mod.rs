mod atomic_task;

pub use self::atomic_task::AtomicTask;

use rt;
use _futures::Future;

pub fn spawn<F>(f: F)
where
    F: Future<Item = (), Error = ()> + 'static,
{
    rt::spawn(move || rt::wait_future(f));
}
