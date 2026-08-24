// SPDX-License-Identifier: Apache-2.0
//! The verdict word, settled in one pure function.
//!
//! ⛔ **A pass word asserts that work was DONE.** `ant`, `tap` and `fin` each shipped a version
//! that could report success having done nothing, and each was found by audit rather than by a
//! test. Reserving `vacuous` costs nothing — every descriptor's `pass_when: {"eq": "applied"}`
//! rejects it automatically — and it is settled here, in one place, so the branch cannot be
//! forgotten at a call site.

/// What the run amounted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Macros were placed and committed to the database.
    Applied,
    /// ⛔ The run did nothing. Never `Applied`.
    Vacuous,
    /// The engine declined: an input did not arrive, or the design needs a capability this
    /// engine does not have (stage 1: a flat cluster needing `par`).
    Refused,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Applied => "applied",
            Status::Vacuous => "vacuous",
            Status::Refused => "refused",
        }
    }

    /// Exit code. ⚠️ **"found something" and "could not run" must never share one.**
    /// `0` applied · `1` refused (the design cannot be processed as asked) · `3` vacuous.
    pub fn exit_code(self) -> u8 {
        match self {
            Status::Applied => 0,
            Status::Refused => 1,
            Status::Vacuous => 3,
        }
    }
}

/// The one place the verdict is decided.
///
/// `placed` is the number of macros actually written to the database. `refusal` is set when the
/// engine declined for a reason the caller must see.
///
/// 🔑 Upstream rule (`ClusteringEngine::init`, `MPL-0017`): a design with no *unfixed* macros makes
/// `run()` return before placing anything. That is not an error — zero can be the right answer —
/// but it must not read as `applied`.
pub fn settle_status(placed: usize, refusal: Option<&str>) -> Status {
    if refusal.is_some() {
        return Status::Refused;
    }
    if placed == 0 {
        return Status::Vacuous;
    }
    Status::Applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placing_nothing_is_never_applied() {
        // The exact shape that shipped broken in ant, tap and fin.
        assert_eq!(settle_status(0, None), Status::Vacuous);
        assert_ne!(settle_status(0, None), Status::Applied);
    }

    #[test]
    fn a_refusal_outranks_the_count() {
        // Refusing after placing some macros is still a refusal: a partial result the caller
        // did not ask for must not be reported as success.
        assert_eq!(settle_status(12, Some("flat cluster needs par")), Status::Refused);
        assert_eq!(settle_status(0, Some("flat cluster needs par")), Status::Refused);
    }

    #[test]
    fn placing_macros_is_applied() {
        assert_eq!(settle_status(1, None), Status::Applied);
    }

    #[test]
    fn no_two_verdicts_share_an_exit_code() {
        let codes = [Status::Applied, Status::Refused, Status::Vacuous].map(Status::exit_code);
        let mut sorted = codes;
        sorted.sort_unstable();
        let mut deduped = sorted.to_vec();
        deduped.dedup();
        assert_eq!(deduped.len(), codes.len(), "a CI gate must distinguish them: {codes:?}");
    }

    #[test]
    fn the_pass_word_is_the_only_one_that_exits_zero() {
        assert_eq!(Status::Applied.exit_code(), 0);
        assert_ne!(Status::Vacuous.exit_code(), 0);
        assert_ne!(Status::Refused.exit_code(), 0);
    }
}
