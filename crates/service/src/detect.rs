use crate::adaptive::{Frame, Ring};
use std::collections::HashMap;

pub struct Finding {
    pub kind: &'static str,
    pub severity: i64,
    pub metric: &'static str,
    pub value: f64,
    pub threshold: f64,
    /// app name to blame, if any
    pub culprit: Option<String>,
    pub detail: String,
}

pub trait Rule {
    fn check(&self, ring: &Ring) -> Option<Finding>;
}

/// Mean of one app metric over the window (frames where the app was absent count as 0).
fn app_avgs(frames: &[&Frame], f: impl Fn(&collector::AppSample) -> f64) -> HashMap<String, f64> {
    let mut sums: HashMap<String, f64> = HashMap::new();
    for fr in frames {
        for a in &fr.apps {
            *sums.entry(a.name.clone()).or_default() += f(a);
        }
    }
    let n = frames.len().max(1) as f64;
    sums.values_mut().for_each(|v| *v /= n);
    sums
}

fn top(m: &HashMap<String, f64>) -> Option<(&String, f64)> {
    m.iter()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(k, v)| (k, *v))
}

/// Whole-machine CPU stays high.
pub struct SystemCpu;
impl Rule for SystemCpu {
    fn check(&self, ring: &Ring) -> Option<Finding> {
        const T: f64 = 90.0;
        let w = ring.window(20)?;
        let avg = w.iter().map(|f| f.sample.cpu_percent).sum::<f64>() / w.len() as f64;
        if avg < T {
            return None;
        }
        let apps = app_avgs(&w, |a| a.cpu_percent);
        let culprit = top(&apps).map(|(n, _)| n.clone());
        Some(Finding {
            kind: "system_cpu_high",
            severity: 2,
            metric: "cpu_percent",
            value: avg,
            threshold: T,
            detail: format!(
                "CPU averaged {avg:.0}% over 20 s; busiest app {}",
                culprit.as_deref().unwrap_or("unknown")
            ),
            culprit,
        })
    }
}

/// One app eats a large share of the CPU.
pub struct AppCpu;
impl Rule for AppCpu {
    fn check(&self, ring: &Ring) -> Option<Finding> {
        const T: f64 = 50.0;
        let w = ring.window(20)?;
        let apps = app_avgs(&w, |a| a.cpu_percent);
        let (name, v) = top(&apps)?;
        (v >= T).then(|| Finding {
            kind: "app_cpu_high",
            severity: 1,
            metric: "app_cpu_percent",
            value: v,
            threshold: T,
            detail: format!("{name} averaged {v:.0}% CPU over 20 s"),
            culprit: Some(name.clone()),
        })
    }
}

/// RAM nearly full.
pub struct MemoryPressure;
impl Rule for MemoryPressure {
    fn check(&self, ring: &Ring) -> Option<Finding> {
        const T: f64 = 90.0;
        let w = ring.window(10)?;
        let pct = w
            .iter()
            .map(|f| {
                f.sample.mem_used_bytes as f64 * 100.0 / f.sample.mem_total_bytes.max(1) as f64
            })
            .sum::<f64>()
            / w.len() as f64;
        if pct < T {
            return None;
        }
        let apps = app_avgs(&w, |a| a.private_bytes as f64);
        let culprit = top(&apps).map(|(n, _)| n.clone());
        Some(Finding {
            kind: "memory_pressure",
            severity: 2,
            metric: "mem_used_percent",
            value: pct,
            threshold: T,
            detail: format!(
                "RAM {pct:.0}% used; largest app {}",
                culprit.as_deref().unwrap_or("unknown")
            ),
            culprit,
        })
    }
}

/// Sustained heavy disk traffic from one app (needs ETW for real numbers).
pub struct DiskHog;
impl Rule for DiskHog {
    fn check(&self, ring: &Ring) -> Option<Finding> {
        const T: f64 = 100.0 * 1024.0 * 1024.0; // bytes/s
        let w = ring.window(10)?;
        let secs = (w.last()?.sample.timestamp - w.first()?.sample.timestamp).max(1) as f64;
        let apps = app_avgs(&w, |a| (a.read_bytes + a.write_bytes) as f64);
        let (name, per_frame) = top(&apps)?;
        let rate = per_frame * w.len() as f64 / secs;
        (rate >= T).then(|| Finding {
            kind: "disk_heavy",
            severity: 1,
            metric: "app_disk_bytes_per_sec",
            value: rate,
            threshold: T,
            detail: format!("{name} moved {:.0} MB/s over 10 s", rate / 1_048_576.0),
            culprit: Some(name.clone()),
        })
    }
}

/// Runs the rules and suppresses repeats for the same (kind, app) during a cooldown.
pub struct Detector {
    rules: Vec<Box<dyn Rule>>,
    last: HashMap<String, u64>,
}

const COOLDOWN_SECS: u64 = 300;

impl Detector {
    pub fn new() -> Self {
        Self {
            rules: vec![
                Box::new(SystemCpu),
                Box::new(AppCpu),
                Box::new(MemoryPressure),
                Box::new(DiskHog),
            ],
            last: HashMap::new(),
        }
    }

    pub fn run(&mut self, ring: &Ring) -> Vec<Finding> {
        let Some(now) = ring.latest().map(|f| f.sample.timestamp) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for r in &self.rules {
            let Some(f) = r.check(ring) else { continue };
            let key = format!("{}:{}", f.kind, f.culprit.as_deref().unwrap_or(""));
            if self.last.get(&key).is_some_and(|t| now < t + COOLDOWN_SECS) {
                continue;
            }
            self.last.insert(key, now);
            out.push(f);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use collector::{AppSample, Sample};

    fn frame(ts: u64, cpu: f64, app_cpu: f64) -> Frame {
        Frame {
            sample: Sample {
                timestamp: ts,
                cpu_percent: cpu,
                mem_used_bytes: 1,
                mem_total_bytes: 100,
                gpu_percent: 0.0,
                vram_used_bytes: 0,
                disk_read_bytes: 0,
                disk_write_bytes: 0,
                net_in_bytes: 0,
                net_out_bytes: 0,
            },
            apps: vec![AppSample {
                name: "hog.exe".into(),
                exe_path: String::new(),
                instances: 1,
                top_pid: 7,
                cpu_percent: app_cpu,
                private_bytes: 1,
                working_set_bytes: 1,
                handles: 1,
                read_bytes: 0,
                write_bytes: 0,
                net_in_bytes: 0,
                net_out_bytes: 0,
                gpu_percent: 0.0,
            }],
        }
    }

    #[test]
    fn fires_once_then_cools_down() {
        let mut ring = Ring::new(30);
        let mut d = Detector::new();
        let mut fired = 0;
        for t in 0..40 {
            ring.push(frame(t, 95.0, 80.0));
            fired += d.run(&ring).len();
        }
        // system_cpu_high + app_cpu_high, each once
        assert_eq!(fired, 2);
    }

    #[test]
    fn quiet_machine_no_findings() {
        let mut ring = Ring::new(30);
        let mut d = Detector::new();
        for t in 0..40 {
            ring.push(frame(t, 5.0, 1.0));
            assert!(d.run(&ring).is_empty());
        }
    }
}
