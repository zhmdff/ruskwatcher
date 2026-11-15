use std::{path::Path, thread, time::Duration};

const FLUSH_EVERY: usize = 5;
const MB: f64 = 1024.0 * 1024.0;

fn main() {
    let mut sampler = match collector::Sampler::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start sampler: {e}");
            return;
        }
    };

    let mut proc_sampler = match collector::ProcessSampler::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start process sampler: {e}");
            return;
        }
    };

    let mut store = match store::Store::open(Path::new("taskwatch.db")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to open database: {e}");
            return;
        }
    };

    let mut buffer = Vec::new();

    loop {
        thread::sleep(Duration::from_secs(2));

        match sampler.sample() {
            Ok(s) => buffer.push(s),
            Err(e) => eprintln!("Sample failed: {e}"),
        }

        match proc_sampler.sample() {
            Ok(mut procs) => {
                procs.sort_by(|a, b| b.cpu_percent.total_cmp(&a.cpu_percent));
                for p in procs.iter().take(5) {
                    println!(
                        "{:<28} pid {:>6}  cpu {:>5.1}%  mem {:>8.1} MB  write {:>8.1} KB",
                        p.name,
                        p.pid,
                        p.cpu_percent,
                        p.private_bytes as f64 / MB,
                        p.write_bytes as f64 / 1024.0
                    );
                }
                println!();
            }
            Err(e) => eprintln!("Process sample failed: {e}"),
        }

        if buffer.len() >= FLUSH_EVERY {
            match store.insert_samples(&buffer) {
                Ok(()) => println!("Saved {} samples", buffer.len()),
                Err(e) => eprintln!("Save failed: {e}"),
            }
            buffer.clear();
        }
    }
}