use crate::model::SessionRecord;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Time(u64),
    Words(usize),
    Focus(usize),
}
pub const MODES: [Mode; 5] = [
    Mode::Time(30),
    Mode::Time(60),
    Mode::Words(25),
    Mode::Words(50),
    Mode::Focus(30),
];
impl Mode {
    pub fn label(self) -> String {
        match self {
            Self::Time(n) => format!("{n}s flow"),
            Self::Words(n) => format!("{n} words"),
            Self::Focus(_) => "focus lab".into(),
        }
    }
    pub fn words(self) -> usize {
        match self {
            Self::Time(_) => 500,
            Self::Words(n) | Self::Focus(n) => n,
        }
    }
}
#[derive(Default, Clone, Debug)]
pub struct Observation {
    pub attempts: u64,
    pub errors: u64,
    pub latency_sum: f64,
    pub timed: u64,
}
#[derive(Debug)]
pub struct Run {
    pub mode: Mode,
    pub target: Vec<char>,
    pub typed: Vec<char>,
    pub started_at: Option<f64>,
    pub finished_at: Option<f64>,
    pub attempts: usize,
    pub errors: usize,
    pub corrections: usize,
    pub observations: BTreeMap<String, Observation>,
    pub trace: Vec<f64>,
    last_key: Option<f64>,
    buckets: Vec<usize>,
}
impl Run {
    pub fn new(mode: Mode, text: String) -> Self {
        Self {
            mode,
            target: text.chars().collect(),
            typed: vec![],
            started_at: None,
            finished_at: None,
            attempts: 0,
            errors: 0,
            corrections: 0,
            observations: BTreeMap::new(),
            trace: vec![],
            last_key: None,
            buckets: vec![],
        }
    }
    pub fn elapsed(&self, now: f64) -> f64 {
        self.started_at.map_or(0., |start| {
            (self.finished_at.unwrap_or(now) - start).max(0.)
        })
    }
    pub fn tick(&mut self, now: f64) {
        if self.started_at.is_none() || self.finished_at.is_some() {
            return;
        }
        if let Mode::Time(seconds) = self.mode
            && self.elapsed(now) >= seconds as f64
        {
            self.finished_at = Some(self.started_at.unwrap() + seconds as f64);
        }
        let seconds = self.elapsed(now).floor() as usize;
        while self.trace.len() < seconds {
            let i = self.trace.len();
            self.trace
                .push(self.buckets.get(i).copied().unwrap_or(0) as f64 * 12.);
        }
    }
    pub fn input(&mut self, c: char, now: f64) {
        self.tick(now);
        if self.finished_at.is_some() || self.typed.len() >= self.target.len() || c.is_control() {
            return;
        }
        if self.started_at.is_none() && c == ' ' {
            return;
        }
        let started = *self.started_at.get_or_insert(now);
        let index = self.typed.len();
        let correct = c == self.target[index];
        let latency = self.last_key.map(|t| ((now - t) * 1000.).clamp(20., 2000.));
        for length in 1..=3.min(index + 1) {
            let start = index + 1 - length;
            if self.typed[start..index] != self.target[start..index] {
                continue;
            }
            let gram: String = self.target[start..=index].iter().collect();
            if gram.contains(' ') {
                continue;
            }
            let o = self.observations.entry(gram).or_default();
            o.attempts += 1;
            o.errors += u64::from(!correct);
            if let Some(ms) = latency {
                o.latency_sum += ms;
                o.timed += 1;
            }
        }
        self.last_key = Some(now);
        self.attempts += 1;
        self.errors += usize::from(!correct);
        let bucket = (now - started).max(0.).floor() as usize;
        self.buckets.resize(self.buckets.len().max(bucket + 1), 0);
        self.buckets[bucket] += usize::from(correct);
        self.typed.push(c);
        if self.typed.len() == self.target.len() {
            self.finished_at = Some(now);
        }
    }
    pub fn backspace(&mut self, word: bool, now: f64) {
        self.tick(now);
        if self.finished_at.is_some() {
            return;
        }
        if self.typed.pop().is_some() {
            self.corrections += 1;
            if word {
                while self.typed.last().is_some_and(|c| *c != ' ') {
                    self.typed.pop();
                    self.corrections += 1;
                }
            }
        }
        self.last_key = None;
    }
    pub fn accuracy(&self) -> f64 {
        if self.attempts == 0 {
            100.
        } else {
            (self.attempts - self.errors) as f64 / self.attempts as f64 * 100.
        }
    }
    pub fn correct(&self) -> usize {
        self.typed
            .iter()
            .zip(&self.target)
            .filter(|(a, b)| a == b)
            .count()
    }
    pub fn wpm(&self, now: f64) -> f64 {
        self.correct() as f64 * 12. / self.elapsed(now).max(1.)
    }
    pub fn progress(&self, now: f64) -> f64 {
        match self.mode {
            Mode::Time(s) => self.elapsed(now) / s as f64,
            _ => self.typed.len() as f64 / self.target.len().max(1) as f64,
        }
        .clamp(0., 1.)
    }
    pub fn record(&self, now: f64, at: u64, targets: Vec<String>) -> SessionRecord {
        let mut trace = self.trace.clone();
        // Include a word session's final partial second, normalized by its duration.
        let partial = self.elapsed(now) - trace.len() as f64;
        if partial > 0.001 {
            let hits = self.buckets.get(trace.len()).copied().unwrap_or(0);
            trace.push(hits as f64 * 12. / partial);
        }
        let mean = if trace.is_empty() {
            0.
        } else {
            trace.iter().sum::<f64>() / trace.len() as f64
        };
        let variance =
            trace.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / trace.len().max(1) as f64;
        let consistency = if mean > 0. {
            (1. - variance.sqrt() / mean).clamp(0., 1.) * 100.
        } else {
            0.
        };
        SessionRecord {
            at,
            mode: self.mode.label(),
            seconds: self.elapsed(now),
            wpm: self.wpm(now),
            raw_wpm: self.attempts as f64 * 12. / self.elapsed(now).max(1.),
            accuracy: self.accuracy(),
            consistency,
            errors: self.errors,
            corrections: self.corrections,
            attempts: self.attempts,
            trace,
            targets,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrections_do_not_erase_accuracy_history() {
        let mut r = Run::new(Mode::Words(1), "cat".into());
        r.input('c', 10.);
        r.input('x', 11.);
        r.backspace(false, 12.);
        r.input('a', 13.);
        r.input('t', 14.);
        assert_eq!(r.errors, 1);
        assert_eq!(r.attempts, 4);
        assert_eq!(r.accuracy(), 75.);
        assert_eq!(r.correct(), 3);
        assert_eq!(r.elapsed(100.), 4.);
        assert_eq!(r.wpm(100.), 9.);
        assert_eq!(r.observations["a"].errors, 1);
    }
    #[test]
    fn timeout_precedes_late_input_and_freezes_metrics() {
        let mut r = Run::new(Mode::Time(30), "hello world".into());
        r.input('h', 5.);
        r.input('e', 35.);
        assert_eq!(r.typed.len(), 1);
        assert_eq!(r.elapsed(100.), 30.);
        assert_eq!(r.trace.len(), 30);
    }
    #[test]
    fn waiting_and_deleting_never_start_clock() {
        let mut r = Run::new(Mode::Words(2), "one two".into());
        r.input(' ', 50.);
        r.backspace(true, 60.);
        r.tick(100.);
        assert!(r.started_at.is_none());
        for (i, c) in "one tw".chars().enumerate() {
            r.input(c, 101. + i as f64);
        }
        r.backspace(true, 110.);
        assert_eq!(r.typed.iter().collect::<String>(), "one ");
    }
    #[test]
    fn bad_prefix_is_not_counted_as_bigram_evidence() {
        let mut r = Run::new(Mode::Words(1), "cat".into());
        r.input('x', 0.);
        r.input('a', 0.2);
        assert!(!r.observations.contains_key("ca"));
        assert!(r.observations.contains_key("a"));
    }
}
