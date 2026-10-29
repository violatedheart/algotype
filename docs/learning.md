# A defensible, inspectable learner

Algotype learns **expected character transitions**, not arbitrary strings of whatever the user happened to type. It stores character, bigram, and trigram observations. Each printable attempt records success/failure against the expected character. For longer contexts, all preceding characters in the context must already match. Backspaces are not successful attempts and do not remove previous errors.

## Evidence and timing

A pattern needs eight attempts before it can become a target. Its lifetime posterior accuracy is:

```
A = (18 + successes) / (20 + attempts)
C = attempts / (attempts + 20)
```

The Beta(18, 2) prior prevents a single mistake from implying complete incompetence. A session's observed accuracy updates a recent EWMA with `alpha = 1 − 0.97^attempts`. Planning blends `0.65 × A + 0.35 × recent_accuracy`, so improvement can change practice without erasing persistent weaknesses.

Timing means the interval from the preceding printable key to the current one, not the whole duration of a bigram/trigram. First keys and keys immediately after deletion have no timing observation. Samples are clamped to 20–2,000 ms to limit terminal batching and interruptions as outliers. An EWMA uses `alpha = 1 − 0.94^timed_samples`. The baseline is the median EWMA of characters with at least eight observations, with a 200 ms fallback. Deliberate pauses still affect session WPM and rhythm even when clipped for the learning model.

The model stores expected-pattern errors, rather than a full matrix of actual-versus-expected substitutions. It therefore targets difficulty reliably but cannot yet distinguish a habitual adjacent-key substitution from other causes of a wrong character.

## Retention and priorities

Each pattern starts with one day of stability. Estimated retention is `R = exp(−days_since_last_seen / stability_days)`. After a review separated by at least twelve hours, 95% or better session accuracy multiplies stability by 1.6; lower accuracy multiplies it by 0.6. Stability stays between 0.5 and 60 days. Short-interval repetitions refresh last-seen time without inflating stability. Retention is a scheduling heuristic, not a calibrated probability of recall.

The priority combines confidence-adjusted deficits:

```
error      = 0 if no mistakes have been observed; otherwise
             clamp((0.98 − blended_accuracy) / 0.18, 0, 1.5)
hesitation = clamp((latency / max(40 ms, baseline) − 1) / 1.5, 0, 1)
priority   = C × (0.60 × error + 0.25 × hesitation + 0.15 × (1 − R))
```

The dominant weighted term supplies the displayed explanation: precision, hesitation, or review. Targets below 0.04 are omitted. Overlapping targets (for example `t`, `th`, and `the`) do not consume all three slots. Only patterns found in the curated vocabulary are selected.

The atlas's separate mastery signal is `100 × A × C × (0.7 + 0.3 × R)`. This rewards sufficient evidence and retention; it is not merely the last session's percentage. The key garden uses color for new, developing, and established keys; the table reports accuracy, latency, and recall explicitly.

## Generation and progression

Exercises use a curated, bundled English vocabulary, never network content or random character soup. Targeted selections are interleaved with independent exploration. The previous eight words are excluded from sampling where possible. Candidates get a modest preference for shorter words, avoiding artificial difficulty inflation.

If a rare target temporarily has no eligible candidates, generation falls back to the general vocabulary. Exploration can also contain targets. The 45% / 70% budgets should therefore not be interpreted as exact final word percentages.

Default words are at most eight letters. Words up to twenty letters unlock once at least three of the most recent five recorded sessions are available, all available sessions in that window have 96% or better accuracy, and mean speed is at least 40 WPM. Accuracy loss automatically restores the gentler vocabulary. These are practical initial thresholds, not universal physiological boundaries.

Only completed exercises update evidence. The selection changes between sessions, never underneath text currently being typed. No history is fabricated on startup.

## Validation and limits

Seeded tests verify that repeated `th` errors change the composition of subsequent natural-word exercises, while noisy two-attempt evidence cannot become a diagnosis. Tests also verify retention decay, spaced stability growth, correction history, and invalid-prefix attribution. Application tests execute generation → typing → analysis → learning → persistence → reopening.

The learner is intentionally modest in its claims. It does not know finger placement, keyboard layout, hand biomechanics, or whether a pause was thought or motor hesitation. It currently trains English words, not punctuation, code, or arbitrary Unicode text. No controlled longitudinal study has established how much this particular scheduler improves typing. Future evaluation should compare accuracy and transition latency on **held-out** vocabulary, alongside a random-practice control group; training-word performance alone would overstate learning.
