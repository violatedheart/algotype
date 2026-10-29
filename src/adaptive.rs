//! Conservative evidence, decaying retention, and diverse natural-word practice.
use crate::{
    engine::Run,
    model::{Profile, Skill},
};
use rand::{
    Rng,
    distributions::{Distribution, WeightedIndex},
};

// Deliberately curated, offline vocabulary. Long words unlock with demonstrated accuracy.
const WORDS: &str = include_str!("../assets/words.txt");
#[derive(Clone, Debug)]
pub struct Focus {
    pub reason: &'static str,
    pub gram: String,
    pub priority: f64,
    pub accuracy: f64,
    pub retention: f64,
    pub latency_ms: f64,
}
#[derive(Clone, Debug)]
pub struct Plan {
    pub text: String,
    pub targets: Vec<String>,
    pub explanation: String,
}

pub fn weaknesses(profile: &Profile, now: u64) -> Vec<Focus> {
    let mut baseline = profile
        .skills
        .iter()
        .filter(|(g, s)| g.len() == 1 && s.attempts >= 8)
        .map(|(_, s)| s.latency_ms)
        .collect::<Vec<_>>();
    baseline.sort_by(f64::total_cmp);
    let baseline = if baseline.is_empty() {
        200.
    } else {
        baseline[baseline.len() / 2]
    };
    let mut ranked: Vec<_> = profile
        .skills
        .iter()
        .filter(|(_, s)| s.attempts >= 8)
        .map(|(g, s)| {
            let accuracy = 0.65 * s.accuracy() + 0.35 * s.recent_accuracy;
            let retention = s.retention(now);
            // A conservative prior expresses uncertainty, not evidence of an actual mistake.
            let error = if s.errors == 0 {
                0.
            } else {
                ((0.98 - accuracy) / 0.18).clamp(0., 1.5)
            };
            let hesitation = ((s.latency_ms / baseline.max(40.) - 1.) / 1.5).clamp(0., 1.);
            let priority =
                s.confidence() * (0.6 * error + 0.25 * hesitation + 0.15 * (1. - retention));
            let reason =
                if 0.6 * error >= 0.25 * hesitation && 0.6 * error >= 0.15 * (1. - retention) {
                    "precision"
                } else if 0.25 * hesitation >= 0.15 * (1. - retention) {
                    "hesitation"
                } else {
                    "review"
                };
            Focus {
                reason,
                gram: g.clone(),
                priority,
                accuracy: accuracy * 100.,
                retention,
                latency_ms: s.latency_ms,
            }
        })
        .collect();
    ranked.sort_by(|a, b| b.priority.total_cmp(&a.priority).then(a.gram.cmp(&b.gram)));
    ranked
}

pub fn plan<R: Rng + ?Sized>(
    profile: &Profile,
    now: u64,
    count: usize,
    focused: bool,
    rng: &mut R,
) -> Plan {
    let ranked = weaknesses(profile, now);
    // Avoid spending all target slots on overlapping versions of the same weakness.
    let mut targets: Vec<String> = vec![];
    for f in &ranked {
        if f.priority < 0.04 {
            continue;
        }
        if targets
            .iter()
            .any(|g| g.contains(&f.gram) || f.gram.contains(g))
        {
            continue;
        }
        if WORDS.split_whitespace().any(|w| w.contains(&f.gram)) {
            targets.push(f.gram.clone());
        }
        if targets.len() == 3 {
            break;
        }
    }
    let recent: Vec<_> = profile.sessions.iter().rev().take(5).collect();
    let ready_for_complex = recent.len() >= 3
        && recent.iter().all(|s| s.accuracy >= 96.)
        && recent.iter().map(|s| s.wpm).sum::<f64>() / recent.len() as f64 >= 40.;
    let words: Vec<&str> = WORDS
        .split_whitespace()
        .filter(|w| w.len() <= if ready_for_complex { 20 } else { 8 })
        .collect();
    let targeted: Vec<bool> = words
        .iter()
        .map(|w| targets.iter().any(|g| w.contains(g)))
        .collect();
    let mut chosen = Vec::with_capacity(count);
    // A fixed exploration budget prevents the learner from trapping users in weak keys.
    for _ in 0..count {
        let exploit = !targets.is_empty() && rng.gen_bool(if focused { 0.7 } else { 0.45 });
        let weights: Vec<f64> = words
            .iter()
            .enumerate()
            .map(|(i, w)| {
                if chosen.iter().rev().take(8).any(|old| old == w) {
                    return 0.;
                }
                if exploit && !targeted[i] {
                    0.
                } else {
                    1. / (1. + (w.len() as f64 - 5.).max(0.) * 0.12)
                }
            })
            .collect();
        let selected = WeightedIndex::new(&weights)
            .map(|d| d.sample(rng))
            .unwrap_or_else(|_| rng.gen_range(0..words.len()));
        chosen.push(words[selected]);
    }
    let explanation = if targets.is_empty() {
        "Calibration · natural words establish a baseline. Accuracy comes before speed.".into()
    } else {
        format!(
            "{} · {}% focus / {}% discovery",
            targets
                .iter()
                .map(|g| format!(
                    "{}: {}",
                    g,
                    ranked.iter().find(|f| f.gram == *g).unwrap().reason
                ))
                .collect::<Vec<_>>()
                .join(" / "),
            if focused { 70 } else { 45 },
            if focused { 30 } else { 55 }
        )
    };
    Plan {
        text: chosen.join(" "),
        targets,
        explanation,
    }
}

pub fn learn(profile: &mut Profile, run: &Run, at: u64) {
    for (gram, o) in &run.observations {
        let s = profile.skills.entry(gram.clone()).or_default();
        let prior_attempts = s.attempts;
        let previous_seen = s.last_seen;
        s.attempts += o.attempts;
        s.errors += o.errors;
        // Per-attempt EWMA, so a large session carries proportionate evidence.
        let alpha = 1. - 0.97_f64.powf(o.attempts as f64);
        let accuracy = 1. - o.errors as f64 / o.attempts.max(1) as f64;
        s.recent_accuracy += (accuracy - s.recent_accuracy) * alpha;
        if o.timed > 0 {
            s.latency_ms += (o.latency_sum / o.timed as f64 - s.latency_ms)
                * (1. - 0.94_f64.powf(o.timed as f64));
        }
        // Stability grows only after successful *spaced* recall, not a burst of repetitions.
        if prior_attempts >= 8 && at.saturating_sub(previous_seen) >= 43200 {
            s.stability_days = if accuracy >= 0.95 {
                (s.stability_days * 1.6).min(60.)
            } else {
                (s.stability_days * 0.6).max(0.5)
            };
        }
        s.last_seen = at;
    }
}
pub fn mastery(s: &Skill, now: u64) -> f64 {
    100. * s.accuracy() * s.confidence() * (0.7 + 0.3 * s.retention(now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};
    #[test]
    fn prior_uncertainty_alone_is_not_a_precision_weakness() {
        let mut p = Profile::default();
        p.skills.insert(
            "a".into(),
            Skill {
                attempts: 12,
                errors: 0,
                ..Skill::default()
            },
        );
        assert_eq!(weaknesses(&p, 0)[0].priority, 0.);
        assert!(
            plan(&p, 0, 25, true, &mut StdRng::seed_from_u64(1))
                .targets
                .is_empty()
        );
    }

    #[test]
    fn complex_vocabulary_requires_sustained_speed_and_accuracy() {
        let mut p = Profile::default();
        let mut rng = StdRng::seed_from_u64(77);
        let easy = plan(&p, 0, 500, false, &mut rng);
        assert!(easy.text.split_whitespace().all(|w| w.len() <= 8));
        let mut sample =
            Run::new(crate::engine::Mode::Words(1), "word".into()).record(1., 0, vec![]);
        sample.wpm = 60.;
        sample.accuracy = 98.;
        p.sessions = vec![sample; 3];
        let advanced = plan(&p, 0, 500, false, &mut rng);
        assert!(advanced.text.split_whitespace().any(|w| w.len() > 8));
        p.sessions.last_mut().unwrap().accuracy = 90.;
        let gentle = plan(&p, 0, 500, false, &mut rng);
        assert!(gentle.text.split_whitespace().all(|w| w.len() <= 8));
    }
    #[test]
    fn noisy_samples_do_not_become_a_diagnosis() {
        let mut p = Profile::default();
        p.skills.insert(
            "th".into(),
            Skill {
                attempts: 2,
                errors: 2,
                ..Skill::default()
            },
        );
        assert!(weaknesses(&p, 0).is_empty());
        assert!(
            plan(&p, 0, 25, true, &mut StdRng::seed_from_u64(1))
                .targets
                .is_empty()
        );
    }
    #[test]
    fn sustained_errors_change_varied_natural_practice() {
        let mut p = Profile::default();
        p.skills.insert(
            "th".into(),
            Skill {
                attempts: 100,
                errors: 40,
                recent_accuracy: 0.6,
                ..Skill::default()
            },
        );
        let mut rng = StdRng::seed_from_u64(4);
        let generated = plan(&p, 0, 500, true, &mut rng);
        assert_eq!(generated.targets, vec!["th"]);
        let words: Vec<_> = generated.text.split_whitespace().collect();
        assert!(words.iter().filter(|w| w.contains("th")).count() > 250);
        assert!(words.windows(2).all(|w| w[0] != w[1]));
        assert!(
            words
                .iter()
                .all(|w| WORDS.split_whitespace().any(|known| known == *w))
        );
    }
    #[test]
    fn retention_decays_and_spaced_success_extends_it() {
        let mut p = Profile::default();
        let s = Skill {
            attempts: 20,
            errors: 0,
            last_seen: 1,
            ..Skill::default()
        };
        assert!(s.retention(864001) < s.retention(1));
        p.skills.insert("a".into(), s);
        let mut r = Run::new(crate::engine::Mode::Words(1), "a".into());
        r.input('a', 0.);
        learn(&mut p, &r, 86401);
        assert!(p.skills["a"].stability_days > 1.);
        let stable = p.skills["a"].stability_days;
        learn(&mut p, &r, 86402);
        assert_eq!(p.skills["a"].stability_days, stable);
    }
}
