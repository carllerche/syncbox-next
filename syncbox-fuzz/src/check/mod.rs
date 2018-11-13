use rt;

pub fn fuzz<F: Fn() + 'static>(f: F)
where
    F: Fn() + Sync + Send + 'static,
{
    rt::check(f);
}

if_futures! {
    use _futures::Future;

    pub fn fuzz_future<F, R>(f: F)
    where
        F: Fn() -> R + Sync + Send + 'static,
        R: Future<Item = (), Error = ()>,
    {
        rt::check(move || {
            rt::wait_future(f());
        });
    }
}
