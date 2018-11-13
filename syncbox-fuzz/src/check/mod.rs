use rt;

pub fn check<F: Fn() + 'static>(f: F)
where
    F: Fn() + Sync + Send + 'static,
{
    rt::check(f);
}
