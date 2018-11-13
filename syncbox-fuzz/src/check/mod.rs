use rt;

pub fn check<F: Fn() + 'static>(f: F)
where
    F: Fn() + Sync + Send + 'static,
{
    rt::check(f);
}

#[cfg(feature = "futures")]
pub use self::futures::*;

#[cfg(feature = "futures")]
mod futures {
    use _futures::Future;

    pub fn check_future<F, R>(f: F)
    where
        F: Fn() -> R + Sync + Send + 'static,
        R: Future<Item = (), Error = ()>,
    {
        unimplemented!();
    }
}
