// Threads, channels, Arc<Mutex<..>>, scoped threads and atomics.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

fn parallel_sum(data: &[u64], parts: usize) -> u64 {
    let chunk = (data.len() + parts - 1) / parts;
    thread::scope(|s| {
        let handles: Vec<_> = data.chunks(chunk).map(|c| s.spawn(move || c.iter().sum::<u64>())).collect();
        handles.into_iter().map(|h| h.join().unwrap()).sum()
    })
}

struct Stats {
    hits: AtomicU64,
    words: Mutex<HashMap<String, usize>>,
}

fn main() {
    let counter = Arc::new(Mutex::new(0));
    let mut handles = vec![];
    for i in 0..8 {
        let counter = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            let mut n = counter.lock().unwrap();
            *n += i;
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!("sum of ids: {}", *counter.lock().unwrap());

    let (tx, rx) = mpsc::channel();
    for id in 0..4 {
        let tx = tx.clone();
        thread::spawn(move || {
            for j in 0..3 {
                tx.send(id * 10 + j).unwrap();
            }
        });
    }
    drop(tx);
    let mut got: Vec<i32> = rx.iter().collect();
    got.sort();
    println!("{:?}", got);

    let data: Vec<u64> = (1..=1000).collect();
    println!("parallel sum {}", parallel_sum(&data, 4));

    let stats = Arc::new(Stats { hits: AtomicU64::new(0), words: Mutex::new(HashMap::new()) });
    let texts = vec!["a b a", "b c", "a"];
    let workers: Vec<_> = texts
        .into_iter()
        .map(|t| {
            let stats = Arc::clone(&stats);
            thread::spawn(move || {
                for w in t.split(' ') {
                    stats.hits.fetch_add(1, Ordering::Relaxed);
                    *stats.words.lock().unwrap().entry(w.to_string()).or_insert(0) += 1;
                }
            })
        })
        .collect();
    for w in workers {
        w.join().expect("worker panicked");
    }
    let words = stats.words.lock().unwrap();
    let mut keys: Vec<_> = words.iter().collect();
    keys.sort();
    println!("{} hits {:?}", stats.hits.load(Ordering::SeqCst), keys);

    let config = Arc::new(RwLock::new(String::from("v1")));
    {
        let mut w = config.write().unwrap();
        w.push_str("-patched");
    }
    let reader = {
        let config = Arc::clone(&config);
        thread::spawn(move || config.read().unwrap().clone())
    };
    println!("{}", reader.join().unwrap());
}
