use collector::AppSample;
use std::{path::Path, thread, time::Duration};

const FLUSH_EVERY: usize = 5;

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

    let mut sys_buffer = Vec::new();
    let mut app_buffer: Vec<(u64, Vec<AppSample>)> = Vec::new();

    loop {
        thread::sleep(Duration::from_secs(2));

        let sample = match sampler.sample() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Sample failed: {e}");
                continue;
            }
        };
        let ts = sample.timestamp;
        sys_buffer.push(sample);

        match proc_sampler.sample() {
            Ok(procs) => app_buffer.push((ts, collector::aggregate(procs))),
            Err(e) => eprintln!("Process sample failed: {e}"),
        }

        if sys_buffer.len() >= FLUSH_EVERY {
            let rows: usize = app_buffer.iter().map(|(_, apps)| apps.len()).sum();
            if let Err(e) = store.insert_samples(&sys_buffer) {
                eprintln!("Saving system samples failed: {e}");
            }
            if let Err(e) = store.insert_apps(&app_buffer) {
                eprintln!("Saving app samples failed: {e}");
            }
            println!("Flushed {} samples, {} app rows", sys_buffer.len(), rows);
            sys_buffer.clear();
            app_buffer.clear();
        }
    }
}
