use rt::{self, Execution, Scheduler};

use serde_json;
use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_MAX_THREADS: usize = 4;

#[derive(Debug)]
pub struct Builder {
    pub max_threads: usize,

    /// When doing an exhaustive fuzz, uses the file to store and load the fuzz
    /// progress
    pub checkpoint_file: Option<PathBuf>,

    /// How often to write the checkpoint file
    pub checkpoint_interval: usize,

    /// What runtime to use
    pub runtime: Runtime,
}

#[derive(Debug)]
pub enum Runtime {
    Thread,
    Generator,
}

impl Builder {
    pub fn new() -> Builder {
        Builder {
            max_threads: DEFAULT_MAX_THREADS,
            checkpoint_file: None,
            checkpoint_interval: 10_000,
            runtime: Runtime::Generator,
        }
    }

    pub fn checkpoint_file(&mut self, file: &str) -> &mut Self {
        self.checkpoint_file = Some(file.into());
        self
    }

    pub fn fuzz<F>(&self, f: F)
    where
        F: Fn() + Sync + Send + 'static,
    {
        let mut execution = Execution::new();
        let mut scheduler = match self.runtime {
            Runtime::Thread => Scheduler::new_thread(self.max_threads),
            Runtime::Generator => Scheduler::new_generator(self.max_threads),
        };

        if let Some(ref path) = self.checkpoint_file {
            if path.exists() {
                let mut file = File::open(path).unwrap();
                let mut contents = String::new();
                file.read_to_string(&mut contents).unwrap();
                execution.branches = serde_json::from_str(&contents).unwrap();
            }
        }

        let f = Arc::new(f);

        let mut i = 0;

        loop {
            i += 1;

            if i % self.checkpoint_interval == 0 {
                println!(" ===== iteration {} =====", i);

                if let Some(ref path) = self.checkpoint_file {
                    let serialized = serde_json::to_string(&execution.branches).unwrap();

                    let mut file = File::create(path).unwrap();
                    file.write_all(serialized.as_bytes()).unwrap();
                }
            }

            let f = f.clone();

            scheduler.run(&mut execution, move || {
                f();
                rt::thread_done();
            });

            if !execution.step() {
                return;
            }
        }
    }
}

pub fn fuzz<F>(f: F)
where
    F: Fn() + Sync + Send + 'static,
{
    Builder::new().fuzz(f)
}

if_futures! {
    use _futures::Future;

    impl Builder {
        pub fn fuzz_future<F, R>(&self, f: F)
        where
            F: Fn() -> R + Sync + Send + 'static,
            R: Future<Item = (), Error = ()>,
        {
            self.fuzz(move || rt::wait_future(f()));
        }
    }

    pub fn fuzz_future<F, R>(f: F)
    where
        F: Fn() -> R + Sync + Send + 'static,
        R: Future<Item = (), Error = ()>,
    {
        Builder::new().fuzz_future(f);
    }
}
