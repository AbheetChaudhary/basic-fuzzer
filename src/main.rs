use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time;
use std::os::unix::process::ExitStatusExt;

const MAX_THREADS: usize = 4; // no. of worker threads
const MUTATIONS: usize = 1e5 as usize; // no. of fuzz cases each thread has to process

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let corpus_names = fs::read_dir("corpus")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<PathBuf>>();

    let mut filedata: Vec<Vec<u8>> = Vec::with_capacity(corpus_names.len());
    for (idx, filename) in corpus_names.iter().enumerate() {
        filedata.push(Vec::new());
        fs::File::open(filename)
            .unwrap()
            .read_to_end(&mut filedata[idx])
            .unwrap();
    }

    let corpus_set: HashSet<Vec<u8>> = HashSet::from_iter(filedata.into_iter());

    let corpus: Vec<Vec<u8>> = Vec::from_iter(corpus_set.into_iter());
    let corpus = Arc::new(corpus);

    let pool = ThreadPool::new(MAX_THREADS);
    for _ in 0..MAX_THREADS * MUTATIONS {
        pool.sender
            .as_ref()
            .unwrap()
            .send(Arc::clone(&corpus))
            .unwrap();
    }

    Ok(())
}

type Data = Arc<Vec<Vec<u8>>>;

struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Data>>,
}

struct Worker {
    thread: Option<thread::JoinHandle<()>>,
}

impl ThreadPool {
    fn new(size: usize) -> Self {
        assert!(size > 0);
        let (tx, rx) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));
        let mut workers = Vec::with_capacity(size);
        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&rx)));
        }

        ThreadPool {
            sender: Some(tx),
            workers,
        }
    }
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Data>>>) -> Self {
        let start_instant = time::Instant::now();
        let mut count = 0;
        let mut fuzz_inp = Vec::new();
        let thread = thread::spawn(move || loop {
            let corpus = receiver.lock().unwrap().recv();
            match corpus {
                Ok(corpus) => {
                    // let input = job();
                    let inp_slice = &corpus.choose(&mut rand::thread_rng()).unwrap()[..];
                    fuzz_inp.clear(); // no need to allocate new for each fuzz case
                    fuzz_inp.extend_from_slice(inp_slice);
                    let inp_len = fuzz_inp.len();
                    for _ in 0..rand::thread_rng().gen_range(0..32) {
                        fuzz_inp[rand::thread_rng().gen_range(0..inp_len)] =
                            rand::thread_rng().gen_range(0..255);
                    }
                    let tmpfn = format!("tmp_{id:02}");
                    fs::write(&tmpfn[..], &fuzz_inp[..]).unwrap();

                    let output = Command::new("objdump")
                        .arg("-x")
                        .arg(tmpfn.as_str())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .output()
                        .expect("Worker {id} failed to execute.");

                    let delta_t = start_instant.elapsed();

                    if output.status.success() {
                        count += 1;
                        println!(
                            "worker_{id:02}\ttime: {:010.4}\tworker case: {count:010} | fcps: {:10.4}",
                            delta_t.as_secs_f32(),
                            count as f64 / delta_t.as_secs() as f64
                        );
                    } else {
                        if let Some(11) = output.status.signal() {
                            println!("SIGSEV");
                        }
                    };
                }
                Err(_) => {
                    break;
                }
            }
        });
        Worker {
            thread: Some(thread),
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        drop(self.sender.take());

        for worker in &mut self.workers {
            worker.thread.take().unwrap().join().unwrap();
        }
    }
}
