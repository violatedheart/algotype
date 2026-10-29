use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skill {
    pub attempts: u64,
    pub errors: u64,
    pub latency_ms: f64,
    pub last_seen: u64,
    pub stability_days: f64,
    pub recent_accuracy: f64,
}
impl Default for Skill {
    fn default() -> Self {
        Self {
            attempts: 0,
            errors: 0,
            latency_ms: 200.,
            last_seen: 0,
            stability_days: 1.,
            recent_accuracy: 0.9,
        }
    }
}
impl Skill {
    pub fn accuracy(&self) -> f64 {
        (18. + (self.attempts - self.errors) as f64) / (20. + self.attempts as f64)
    }
    pub fn confidence(&self) -> f64 {
        self.attempts as f64 / (self.attempts as f64 + 20.)
    }
    pub fn retention(&self, now: u64) -> f64 {
        let days = now.saturating_sub(self.last_seen) as f64 / 86400.;
        (-days / self.stability_days.max(0.5)).exp()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionRecord {
    pub at: u64,
    pub mode: String,
    pub seconds: f64,
    pub wpm: f64,
    pub raw_wpm: f64,
    pub accuracy: f64,
    pub consistency: f64,
    pub errors: usize,
    pub corrections: usize,
    pub attempts: usize,
    pub trace: Vec<f64>,
    pub targets: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Preferences {
    pub theme: usize,
    pub reduced_motion: bool,
    pub mode: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub schema: u32,
    pub sessions: Vec<SessionRecord>,
    pub skills: BTreeMap<String, Skill>,
    pub preferences: Preferences,
    pub total_sessions: u64,
    pub best_wpm: f64,
    pub best_by_mode: BTreeMap<String, f64>,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            schema: 1,
            sessions: vec![],
            skills: BTreeMap::new(),
            preferences: Preferences::default(),
            total_sessions: 0,
            best_wpm: 0.,
            best_by_mode: BTreeMap::new(),
        }
    }
}
impl Profile {
    pub fn best(&self) -> f64 {
        self.sessions
            .iter()
            .map(|s| s.wpm)
            .fold(self.best_wpm, f64::max)
    }
    pub fn best_for(&self, mode: &str) -> f64 {
        self.sessions
            .iter()
            .filter(|s| s.mode == mode)
            .map(|s| s.wpm)
            .fold(self.best_by_mode.get(mode).copied().unwrap_or(0.), f64::max)
    }
    pub fn streak(&self, now: u64) -> usize {
        let days: std::collections::BTreeSet<_> =
            self.sessions.iter().map(|s| s.at / 86400).collect();
        let today = now / 86400;
        let mut day = if days.contains(&today) {
            today
        } else {
            today.saturating_sub(1)
        };
        let mut count = 0;
        while days.contains(&day) {
            count += 1;
            if day == 0 {
                break;
            }
            day -= 1;
        }
        count
    }
}
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifetime_bests_survive_a_compacted_history() {
        let mut p = Profile {
            best_wpm: 95.,
            ..Profile::default()
        };
        p.best_by_mode.insert("30s flow".into(), 95.);
        assert_eq!(p.best(), 95.);
        assert_eq!(p.best_for("30s flow"), 95.);
        assert_eq!(p.best_for("25 words"), 0.);
        let roundtrip: Profile = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(roundtrip.best(), 95.);
    }
}
