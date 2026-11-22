use collector::{AppSample, Sample};
use std::collections::VecDeque;
use std::time::Duration;

/// Sampling state: slow when idle, faster when the machine is busy.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Idle,
    Normal,
    Spike,
}

const SPIKE_ENTER: f64 = 80.0;
const SPIKE_EXIT: f64 = 60.0;
const IDLE_BELOW: f64 = 8.0;

impl Mode {
    pub fn interval(self) -> Duration {
        Duration::from_secs(match self {
            Mode::Idle => 5,
            Mode::Normal => 2,
            Mode::Spike => 1,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Idle => "idle",
            Mode::Normal => "normal",
            Mode::Spike => "spike",
        }
    }

    /// Hysteresis on the spike edge so we do not flap around the threshold.
    pub fn next(self, cpu: f64) -> Mode {
        if cpu >= SPIKE_ENTER || (self == Mode::Spike && cpu >= SPIKE_EXIT) {
            Mode::Spike
        } else if cpu < IDLE_BELOW {
            Mode::Idle
        } else {
            Mode::Normal
        }
    }
}

#[derive(Clone)]
pub struct Frame {
    pub sample: Sample,
    pub apps: Vec<AppSample>,
}

/// Last `span` seconds of frames, kept in memory for spike snapshots.
pub struct Ring {
    frames: VecDeque<Frame>,
    span: u64,
}

impl Ring {
    pub fn new(span: u64) -> Self {
        Self {
            frames: VecDeque::new(),
            span,
        }
    }

    pub fn push(&mut self, f: Frame) {
        let now = f.sample.timestamp;
        self.frames.push_back(f);
        while self
            .frames
            .front()
            .is_some_and(|f| f.sample.timestamp + self.span < now)
        {
            self.frames.pop_front();
        }
    }

    pub fn latest(&self) -> Option<&Frame> {
        self.frames.back()
    }

    pub fn span_secs(&self) -> u64 {
        match (self.frames.front(), self.frames.back()) {
            (Some(a), Some(b)) => b.sample.timestamp - a.sample.timestamp,
            _ => 0,
        }
    }

    /// Frames in the last `secs`, or None until the ring covers at least 75% of that window.
    pub fn window(&self, secs: u64) -> Option<Vec<&Frame>> {
        let now = self.latest()?.sample.timestamp;
        let v: Vec<&Frame> = self
            .frames
            .iter()
            .filter(|f| f.sample.timestamp + secs >= now)
            .collect();
        let covered = now - v.first()?.sample.timestamp;
        (covered * 4 >= secs * 3).then_some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ts: u64, cpu: f64) -> Frame {
        Frame {
            sample: Sample {
                timestamp: ts,
                cpu_percent: cpu,
                mem_used_bytes: 0,
                mem_total_bytes: 1,
                gpu_percent: 0.0,
                vram_used_bytes: 0,
                disk_read_bytes: 0,
                disk_write_bytes: 0,
                net_in_bytes: 0,
                net_out_bytes: 0,
            },
            apps: vec![],
        }
    }

    #[test]
    fn mode_hysteresis() {
        assert_eq!(Mode::Normal.next(85.0), Mode::Spike);
        assert_eq!(Mode::Spike.next(70.0), Mode::Spike);
        assert_eq!(Mode::Spike.next(50.0), Mode::Normal);
        assert_eq!(Mode::Normal.next(2.0), Mode::Idle);
    }

    #[test]
    fn ring_drops_old_and_windows() {
        let mut r = Ring::new(30);
        for t in 0..=60 {
            r.push(frame(t, 1.0));
        }
        assert_eq!(r.span_secs(), 30);
        assert!(r.window(20).is_some());
        let mut short = Ring::new(30);
        short.push(frame(0, 1.0));
        short.push(frame(2, 1.0));
        assert!(short.window(20).is_none());
    }
}
