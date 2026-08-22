//! Runs tools off the UI thread.
//!
//! On a 5 MB paste the JSON formatter takes ~70 ms per run, and it runs on
//! every keystroke. Doing that on the UI thread drops frames on each key.
//! Instead, one long-lived thread owns execution: the UI submits jobs, the
//! worker always skips ahead to the newest queued job (so ten fast keystrokes
//! cost one run, not ten), and results tagged with a stale generation are
//! dropped on receipt.
//!
//! The worker knows nothing about egui. It takes an `on_done` callback, which
//! the app uses to request a repaint — a background thread finishing does not
//! otherwise wake an on-demand renderer.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use rustafari_core::{Input, Options, Tool, ToolResult};

struct Job {
    generation: u64,
    tool: Arc<dyn Tool>,
    /// Owned copies: the worker outlives the frame that submitted them.
    left: String,
    right: String,
    options: Options,
}

struct Done {
    generation: u64,
    result: ToolResult,
}

pub struct Worker {
    jobs: Sender<Job>,
    results: Receiver<Done>,
    /// The most recent submission. Only a result carrying this generation is
    /// current; anything older describes input the user has already changed.
    generation: u64,
    pending: bool,
}

impl Worker {
    pub fn spawn(on_done: impl Fn() + Send + 'static) -> Self {
        let (jobs, job_rx) = mpsc::channel::<Job>();
        let (done_tx, results) = mpsc::channel::<Done>();

        thread::Builder::new()
            .name("rustafari-worker".into())
            .spawn(move || {
                // `recv` blocks until there is work; an error means the app
                // dropped its end and we should exit with it.
                while let Ok(mut job) = job_rx.recv() {
                    // Coalesce: everything queued behind this job is already
                    // superseded, so jump straight to the newest.
                    while let Ok(newer) = job_rx.try_recv() {
                        job = newer;
                    }

                    let result = job
                        .tool
                        .run(Input::pair(&job.left, &job.right), &job.options);
                    if done_tx
                        .send(Done {
                            generation: job.generation,
                            result,
                        })
                        .is_err()
                    {
                        return;
                    }
                    on_done();
                }
            })
            .expect("spawn worker thread");

        Worker {
            jobs,
            results,
            generation: 0,
            pending: false,
        }
    }

    pub fn submit(&mut self, tool: Arc<dyn Tool>, left: String, right: String, options: Options) {
        self.generation += 1;
        self.pending = true;
        // A send error means the worker thread died, which only happens if a
        // tool panicked. The UI keeps showing the last good output; there is
        // nothing more useful to do here than not crash as well.
        let _ = self.jobs.send(Job {
            generation: self.generation,
            tool,
            left,
            right,
            options,
        });
    }

    /// Collects finished work. Returns the newest result if it corresponds to
    /// the latest submission; older results are discarded silently.
    pub fn poll(&mut self) -> Option<ToolResult> {
        // Results arrive in submission order, so only the last one matters.
        let newest = self.results.try_iter().last()?;
        if newest.generation == self.generation {
            self.pending = false;
            Some(newest.result)
        } else {
            None
        }
    }

    /// True while a submission is outstanding, so the UI can show it.
    pub fn is_pending(&self) -> bool {
        self.pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustafari_core::{Category, ToolMeta};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    /// Echoes its input after a delay, counting how many times it ran.
    struct SlowEcho {
        runs: Arc<AtomicUsize>,
        delay: Duration,
    }

    impl Tool for SlowEcho {
        fn meta(&self) -> ToolMeta {
            ToolMeta {
                id: "slow-echo",
                name: "Slow Echo",
                category: Category::Text,
                description: "",
                keywords: &[],
            }
        }

        fn run(&self, input: Input<'_>, _: &Options) -> ToolResult {
            self.runs.fetch_add(1, Ordering::SeqCst);
            thread::sleep(self.delay);
            Ok(input.left.to_owned())
        }
    }

    fn wait_for(worker: &mut Worker, timeout: Duration) -> Option<ToolResult> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Some(result) = worker.poll() {
                return Some(result);
            }
            thread::sleep(Duration::from_millis(2));
        }
        None
    }

    fn slow(delay_ms: u64) -> (Arc<dyn Tool>, Arc<AtomicUsize>) {
        let runs = Arc::new(AtomicUsize::new(0));
        let tool: Arc<dyn Tool> = Arc::new(SlowEcho {
            runs: runs.clone(),
            delay: Duration::from_millis(delay_ms),
        });
        (tool, runs)
    }

    #[test]
    fn delivers_a_result_and_clears_pending() {
        let mut worker = Worker::spawn(|| {});
        let (tool, _) = slow(0);

        assert!(!worker.is_pending());
        worker.submit(tool, "hello".into(), String::new(), Options::default());
        assert!(worker.is_pending());

        let result = wait_for(&mut worker, Duration::from_secs(2)).expect("result");
        assert_eq!(result.unwrap(), "hello");
        assert!(!worker.is_pending());
    }

    #[test]
    fn coalesces_a_burst_of_submissions_into_few_runs() {
        let mut worker = Worker::spawn(|| {});
        let (tool, runs) = slow(30);

        // Ten "keystrokes" faster than one run can finish.
        for i in 0..10 {
            worker.submit(
                tool.clone(),
                format!("v{i}"),
                String::new(),
                Options::default(),
            );
        }

        let result = wait_for(&mut worker, Duration::from_secs(5)).expect("result");
        assert_eq!(result.unwrap(), "v9", "must reflect the newest input");

        // The first job may already have started before the rest were queued,
        // so at most two runs: the one in flight, then the newest.
        let count = runs.load(Ordering::SeqCst);
        assert!(
            count <= 2,
            "expected coalescing, but tool ran {count} times"
        );
    }

    #[test]
    fn stale_results_are_never_surfaced() {
        let mut worker = Worker::spawn(|| {});
        let (tool, _) = slow(0);

        worker.submit(
            tool.clone(),
            "old".into(),
            String::new(),
            Options::default(),
        );
        // Let the first finish before the second is even submitted.
        thread::sleep(Duration::from_millis(50));
        worker.submit(tool, "new".into(), String::new(), Options::default());

        // Every poll from here on must be either nothing or "new" — never "old".
        let start = Instant::now();
        let mut saw_new = false;
        while start.elapsed() < Duration::from_secs(2) && !saw_new {
            if let Some(result) = worker.poll() {
                assert_eq!(result.unwrap(), "new");
                saw_new = true;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(saw_new);
    }

    #[test]
    fn calls_on_done_after_each_completed_run() {
        let dones = Arc::new(AtomicUsize::new(0));
        let counter = dones.clone();
        let mut worker = Worker::spawn(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        let (tool, _) = slow(0);

        worker.submit(tool, "x".into(), String::new(), Options::default());
        wait_for(&mut worker, Duration::from_secs(2))
            .expect("result")
            .unwrap();

        // The result reaching the channel does not mean `on_done` has run:
        // the worker sends first and calls back second, deliberately, so the
        // renderer it wakes cannot poll an empty channel. Asserting the count
        // the instant the result arrives therefore raced the worker thread,
        // and did intermittently fail. Wait for the callback instead.
        let start = Instant::now();
        while dones.load(Ordering::SeqCst) == 0 && start.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(dones.load(Ordering::SeqCst), 1);
    }
}
