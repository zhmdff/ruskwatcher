use std::{thread, time::Duration};

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

fn main() {
    let mut sampler = match collector::Sampler::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start sampler: {e}");
            return;
        }
    };

    loop {
        thread::sleep(Duration::from_secs(2));
        match sampler.sample() {
            Ok(s) => println!(
                "{} CPU {:.1}% RAM {:.1}/{:.1} GB",
                s.timestamp,
                s.cpu_percent,
                s.mem_used_bytes as f64 / GIB,
                s.mem_total_bytes as f64 / GIB
            ),
            Err(e) => eprintln!("Sample failed: {e}"),
        }
    }
}