// A pure implementation of the FSRS-6 scheduler: no I/O, no vault, just the
// memory-model formulas. The review extension owns the page state and decides
// which grade maps to which rating; this module only turns (state, elapsed,
// rating) into the next state and a stability into an interval.
//
// Parameters and formulas follow open-spaced-repetition/fsrs-rs
// (`DEFAULT_PARAMETERS` and `power_forgetting_curve` / `next_interval` /
// `step` in src/model.rs). Tests below check the results against those
// reference formulas.

/// The official FSRS-6 default parameters (w0..w20), as published by fsrs-rs.
/// Fits the average person's learning habits.
pub const DEFAULT_PARAMETERS: [f64; 21] = [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835,
    0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
];

/// Stability is measured in days and clamped to the same range fsrs-rs uses.
const S_MIN: f64 = 0.001;
const S_MAX: f64 = 36500.0;
/// Difficulty is on a 1..=10 scale.
const D_MIN: f64 = 1.0;
const D_MAX: f64 = 10.0;

/// A page's FSRS memory state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Memory {
    /// Stability: days at which recall probability drops to 90%.
    pub stability: f64,
    /// Difficulty, 1 (easy) to 10 (hard).
    pub difficulty: f64,
}

/// A grade as an FSRS rating: 1 again, 2 hard, 3 good, 4 easy.
pub type Rating = u8;

/// Forgetting-curve decay: `-w20`.
fn decay() -> f64 {
    -DEFAULT_PARAMETERS[20]
}

/// Horizontal scale of the forgetting curve: `0.9^(1/decay) - 1`.
fn factor() -> f64 {
    (0.9f64.ln() / decay()).exp() - 1.0
}

/// Probability that a page with `stability` days is still recalled after
/// `elapsed_days`: the FSRS power forgetting curve `R(t, S)`.
pub fn retrievability(elapsed_days: f64, stability: f64) -> f64 {
    (elapsed_days / stability * factor() + 1.0).powf(decay())
}

/// The interval, in days, at which retrievability falls to `desired_retention`
/// for a page with `stability` days. At the default 0.9 this equals `stability`.
pub fn interval_days(stability: f64, desired_retention: f64) -> f64 {
    stability / factor() * (desired_retention.powf(1.0 / decay()) - 1.0)
}

/// Initial stability for a grade: `w0`..`w3`.
fn init_stability(rating: Rating) -> f64 {
    DEFAULT_PARAMETERS[(rating.clamp(1, 4) as usize) - 1]
}

/// Initial difficulty for a grade: `w4 - exp(w5 * (rating - 1)) + 1`.
fn init_difficulty(rating: Rating) -> f64 {
    DEFAULT_PARAMETERS[4] - (DEFAULT_PARAMETERS[5] * (rating.clamp(1, 4) as f64 - 1.0)).exp() + 1.0
}

/// Difficulty after a review, before mean reversion.
fn next_difficulty(difficulty: f64, rating: Rating) -> f64 {
    let delta_d = -DEFAULT_PARAMETERS[6] * (rating as f64 - 3.0);
    let damping = (10.0 - difficulty) * delta_d / 9.0;
    difficulty + damping
}

/// Pull difficulty back toward the value a fresh "easy" page would have.
fn mean_reversion(new_d: f64) -> f64 {
    DEFAULT_PARAMETERS[7] * (init_difficulty(4) - new_d) + new_d
}

/// Stability after a successful recall, with the hard penalty and easy bonus.
fn stability_after_success(last_s: f64, last_d: f64, r: f64, rating: Rating) -> f64 {
    let hard_penalty = if rating == 2 {
        DEFAULT_PARAMETERS[15]
    } else {
        1.0
    };
    let easy_bonus = if rating == 4 {
        DEFAULT_PARAMETERS[16]
    } else {
        1.0
    };
    last_s
        * (DEFAULT_PARAMETERS[8].exp()
            * (11.0 - last_d)
            * last_s.powf(-DEFAULT_PARAMETERS[9])
            * (((1.0 - r) * DEFAULT_PARAMETERS[10]).exp() - 1.0)
            * hard_penalty
            * easy_bonus
            + 1.0)
}

/// Stability after a lapse (`again`), never above the pre-review stability.
fn stability_after_failure(last_s: f64, last_d: f64, r: f64) -> f64 {
    let new_s = DEFAULT_PARAMETERS[11]
        * last_d.powf(-DEFAULT_PARAMETERS[12])
        * ((last_s + 1.0).powf(DEFAULT_PARAMETERS[13]) - 1.0)
        * ((1.0 - r) * DEFAULT_PARAMETERS[14]).exp();
    let ceiling = last_s / (DEFAULT_PARAMETERS[17] * DEFAULT_PARAMETERS[18]).exp();
    new_s.min(ceiling)
}

/// Stability update for a review on the same day (`elapsed_days == 0`).
fn stability_short_term(last_s: f64, rating: Rating) -> f64 {
    let sinc = (DEFAULT_PARAMETERS[17] * (rating as f64 - 3.0 + DEFAULT_PARAMETERS[18])).exp()
        * last_s.powf(-DEFAULT_PARAMETERS[19]);
    last_s * if rating >= 2 { sinc.max(1.0) } else { sinc }
}

/// The memory state after reviewing a page with grade `rating`, `elapsed_days`
/// since its last review.
///
/// `current` is `None` when the page has no FSRS state yet (a new page, or one
/// last reviewed before adaptive scheduling); the state is then initialized
/// from the grade instead of updated. A same-day review uses the short-term
/// stability update, as in fsrs-rs.
pub fn next_memory(current: Option<Memory>, elapsed_days: f64, rating: Rating) -> Memory {
    let rating = rating.clamp(1, 4);
    let Some(state) = current else {
        return Memory {
            stability: init_stability(rating).clamp(S_MIN, S_MAX),
            difficulty: init_difficulty(rating).clamp(D_MIN, D_MAX),
        };
    };

    let last_s = state.stability.clamp(S_MIN, S_MAX);
    let last_d = state.difficulty.clamp(D_MIN, D_MAX);
    let r = retrievability(elapsed_days, last_s);

    let mut new_s = if rating == 1 {
        stability_after_failure(last_s, last_d, r)
    } else {
        stability_after_success(last_s, last_d, r, rating)
    };
    if elapsed_days == 0.0 {
        new_s = stability_short_term(last_s, rating);
    }

    let new_d = mean_reversion(next_difficulty(last_d, rating)).clamp(D_MIN, D_MAX);
    Memory {
        stability: new_s.clamp(S_MIN, S_MAX),
        difficulty: new_d,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_stability_is_w0_to_w3() {
        for (i, rating) in [1u8, 2, 3, 4].into_iter().enumerate() {
            assert_eq!(init_stability(rating), DEFAULT_PARAMETERS[i]);
            assert_eq!(
                next_memory(None, 0.0, rating).stability,
                DEFAULT_PARAMETERS[i]
            );
        }
    }

    #[test]
    fn initial_difficulty_follows_the_reference_formula() {
        for rating in 1u8..=4 {
            let expected =
                DEFAULT_PARAMETERS[4] - (DEFAULT_PARAMETERS[5] * (rating as f64 - 1.0)).exp() + 1.0;
            assert_eq!(
                next_memory(None, 0.0, rating).difficulty,
                expected.clamp(D_MIN, D_MAX)
            );
        }
    }

    #[test]
    fn interval_at_desired_retention_equals_stability_when_default() {
        for s in [0.212, 1.0, 5.0, 121.01552] {
            assert!(
                (interval_days(s, 0.9) - s).abs() < 1e-6,
                "interval({s}) = {}",
                interval_days(s, 0.9)
            );
        }
    }

    #[test]
    fn higher_desired_retention_shortens_intervals() {
        let short = interval_days(10.0, 0.97);
        let mid = interval_days(10.0, 0.9);
        let long = interval_days(10.0, 0.7);
        assert!(short < mid && mid < long, "{short} {mid} {long}");
    }

    #[test]
    fn retrievability_matches_its_own_interval() {
        assert!((retrievability(0.0, 5.0) - 1.0).abs() < 1e-12);
        for r in [0.7, 0.8, 0.9, 0.95] {
            let interval = interval_days(5.0, r);
            assert!(
                (retrievability(interval, 5.0) - r).abs() < 1e-9,
                "R at the interval for {r} was {}",
                retrievability(interval, 5.0)
            );
        }
    }

    #[test]
    fn difficulty_is_clamped_to_one_and_ten() {
        // Easy's raw initial difficulty (w4 - exp(3*w5) + 1) is below 1.
        assert_eq!(next_memory(None, 0.0, 4).difficulty, D_MIN);
        let extremes = [
            Memory {
                stability: 50.0,
                difficulty: D_MIN,
            },
            Memory {
                stability: 50.0,
                difficulty: D_MAX,
            },
        ];
        for rating in 1u8..=4 {
            for state in extremes {
                let d = next_memory(Some(state), 5.0, rating).difficulty;
                assert!((D_MIN..=D_MAX).contains(&d), "difficulty {d}");
            }
        }
    }

    #[test]
    fn good_grows_stability_and_again_shrinks_it() {
        let before = Memory {
            stability: 5.0,
            difficulty: 5.0,
        };
        let good = next_memory(Some(before), 5.0, 3);
        let again = next_memory(Some(before), 5.0, 1);
        assert!(good.stability > before.stability, "{good:?}");
        assert!(again.stability < before.stability, "{again:?}");
        assert!(good.stability > again.stability);
    }

    #[test]
    fn review_order_is_hard_below_good_below_easy() {
        let state = Memory {
            stability: 10.0,
            difficulty: 5.0,
        };
        let hard = next_memory(Some(state), 10.0, 2).stability;
        let good = next_memory(Some(state), 10.0, 3).stability;
        let easy = next_memory(Some(state), 10.0, 4).stability;
        assert!(hard <= good && good <= easy, "{hard} {good} {easy}");
    }

    #[test]
    fn same_day_review_uses_the_short_term_update() {
        let state = Memory {
            stability: 5.0,
            difficulty: 5.0,
        };
        let short = next_memory(Some(state), 0.0, 3);
        let long = next_memory(Some(state), 1.0, 3);
        assert_eq!(
            short.stability,
            stability_short_term(5.0, 3).clamp(S_MIN, S_MAX)
        );
        assert_ne!(short.stability, long.stability);
    }
}
