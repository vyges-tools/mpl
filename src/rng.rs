// SPDX-License-Identifier: Apache-2.0
//! The random numbers the annealing search draws, reproduced exactly.
//!
//! 🔑 **Why this exists at all.** The tiling search is a simulated annealer, so its result is a
//! function of its random stream: reproducing the algorithm without reproducing the stream
//! reproduces nothing. `std::mt19937` is specified by the C++ standard and portable by
//! construction, but `std::uniform_real_distribution` is **not** — the standard fixes its
//! distribution, not its mapping from generator output to value, and every library is free to
//! differ.
//!
//! ⚠️ **Measured, not assumed.** The vectors in `tests/rng.rs` were produced by compiling a probe
//! against the reference toolchain and printing raw bit patterns, and they came out **identical
//! under GCC 11.4 and GCC 13.3** — so the mapping below is not a guess about one library version.

/// `std::mt19937`, the 32-bit Mersenne Twister the annealer is seeded with.
///
/// ℹ️ Fully specified by the standard, so this is a transcription rather than a reconstruction.
#[derive(Debug, Clone)]
pub struct Mt19937 {
    state: [u32; Self::N],
    index: usize,
}

impl Mt19937 {
    const N: usize = 624;
    const M: usize = 397;
    const MATRIX_A: u32 = 0x9908_b0df;
    const UPPER_MASK: u32 = 0x8000_0000;
    const LOWER_MASK: u32 = 0x7fff_ffff;

    pub fn new(seed: u32) -> Self {
        let mut state = [0u32; Self::N];
        state[0] = seed;
        for i in 1..Self::N {
            let prev = state[i - 1];
            // ⚠️ Wrapping throughout: the recurrence is defined modulo 2^32.
            state[i] = 1_812_433_253u32
                .wrapping_mul(prev ^ (prev >> 30))
                .wrapping_add(i as u32);
        }
        Self { state, index: Self::N }
    }

    fn twist(&mut self) {
        for i in 0..Self::N {
            let x = (self.state[i] & Self::UPPER_MASK)
                | (self.state[(i + 1) % Self::N] & Self::LOWER_MASK);
            let mut x_a = x >> 1;
            if x & 1 != 0 {
                x_a ^= Self::MATRIX_A;
            }
            self.state[i] = self.state[(i + Self::M) % Self::N] ^ x_a;
        }
        self.index = 0;
    }

    /// One 32-bit output, tempered.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        if self.index >= Self::N {
            self.twist();
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }
}

/// The value `std::uniform_real_distribution<float>(0.0, 1.0)` yields from one `mt19937`.
///
/// 🔑 **Exactly ONE generator draw per value**, measured: after taking one float, the generator's
/// next output is the reference stream's second word. That matters more than the value itself —
/// the annealer interleaves these with Boost integer draws on the SAME generator, so consuming a
/// different number of words desynchronises everything downstream even if each value is right.
///
/// ⚠️ **The division is in `f32`, not `f64`.** `generate_canonical` does its arithmetic in the
/// result type, so the 32-bit word is first ROUNDED to 24 bits of mantissa and only then scaled.
/// Computing in `f64` and narrowing gives a different value for most inputs.
///
/// ⚠️ The `>= 1.0` case is real: a word near `2^32` rounds up to exactly `2^32`, which would
/// return 1.0 and put the value outside the half-open range the callers assume.
pub fn canonical_f32(rng: &mut Mt19937) -> f32 {
    let value = rng.next() as f32 / 4_294_967_296.0f32;
    if value >= 1.0 {
        // The largest float below 1.0 — upstream reaches it through `nextafter`.
        f32::from_bits(0x3f7f_ffff)
    } else {
        value
    }
}

/// The value `boost::random::uniform_int_distribution<>(0, n - 1)` yields from one `mt19937`.
///
/// 🔑 **A bucket DIVISION with rejection, not a multiply-and-shift.** The two agree almost
/// everywhere, which is exactly what makes the difference dangerous: a 200,000-draw sample at
/// `n = 100` showed no disagreement at all, while `n = 1000` disagreed 5 times — always by one,
/// always upward. Sampling would have "confirmed" the wrong formula.
///
/// ⚠️ **The bucket is over `2^32 - 1`, not `2^32`**, because Boost works in the generator's
/// *range* (`max - min`) rather than its cardinality. The `+= 1` correction restores the case
/// where the range divides exactly; dropping it doubles every value for `n = 2`.
///
/// ⚠️ The rejection loop consumes an EXTRA generator word when it fires. With a 32-bit generator
/// its probability is at most `n / 2^32`, so it essentially never fires for the handful of macros
/// a cluster holds — but "essentially never" is not never, and a desynchronised generator would
/// corrupt every later draw rather than one value.
pub fn uniform_int(rng: &mut Mt19937, n: u32) -> u32 {
    debug_assert!(n > 0, "an empty range has no value to return");
    if n == 1 {
        // `range == 0`: upstream returns the minimum without touching the generator.
        return 0;
    }
    const BRANGE: u64 = u32::MAX as u64;
    let range = (n - 1) as u64;
    let mut bucket = BRANGE / (range + 1);
    if BRANGE % (range + 1) == range {
        bucket += 1;
    }
    loop {
        let result = rng.next() as u64 / bucket;
        if result <= range {
            return result as u32;
        }
    }
}
