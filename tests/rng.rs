// SPDX-License-Identifier: Apache-2.0
//! The annealer's random stream, against the reference.
//!
//! 🔑 **Every vector here was MEASURED, not derived.** A probe was compiled against the reference
//! toolchain and printed raw bit patterns for four seeds; these are those bytes. Deriving them
//! from this crate's own implementation would make the test a mirror and prove nothing.
//!
//! ⚠️ The same probe was compiled under **GCC 11.4 and GCC 13.3** and produced identical output,
//! which is what makes `std::uniform_real_distribution<float>` — implementation-defined by the
//! standard — safe to depend on here.

use vyges_mpl::rng::{canonical_f32, Mt19937};

/// `(seed, first eight raw outputs)`.
const RAW: [(u32, [u32; 8]); 4] = [
    (
        0,
        [
            0x8c7f0aac, 0x97c4aa2f, 0xb716a675, 0xd821ccc0, 0x9a4eb343, 0xdba252fb, 0x8b7d76c3,
            0xd8e57d67,
        ],
    ),
    (
        1,
        [
            0x6ac1f425, 0xff4780eb, 0xb8672f8c, 0xeebc1448, 0x00077eff, 0x20ccc389, 0x4d65aacb,
            0xffc11e85,
        ],
    ),
    (
        42,
        [
            0x5fe1dc66, 0xcbea3db3, 0xf362035c, 0x2ef5950e, 0xbb63f46a, 0xc799d447, 0x9941aebc,
            0x98cb2c14,
        ],
    ),
    (
        123456789,
        [
            0x8867beb8, 0xfd9b2e9c, 0x88bd2d32, 0x035e17d9, 0x8272115a, 0xb94f431d, 0xb6ac21ec,
            0x2296c307,
        ],
    ),
];

/// `(seed, bit patterns of the first eight floats)`.
const FLOATS: [(u32, [u32; 8]); 4] = [
    (
        0,
        [
            0x3f0c7f0b, 0x3f17c4aa, 0x3f3716a6, 0x3f5821cd, 0x3f1a4eb3, 0x3f5ba253, 0x3f0b7d77,
            0x3f58e57d,
        ],
    ),
    (
        1,
        [
            0x3ed583e8, 0x3f7f4781, 0x3f386730, 0x3f6ebc14, 0x38efdfe0, 0x3e03330e, 0x3e9acb56,
            0x3f7fc11f,
        ],
    ),
    (
        42,
        [
            0x3ebfc3b9, 0x3f4bea3e, 0x3f736203, 0x3e3bd654, 0x3f3b63f4, 0x3f4799d4, 0x3f1941af,
            0x3f18cb2c,
        ],
    ),
    (
        123456789,
        [
            0x3f0867bf, 0x3f7d9b2f, 0x3f08bd2d, 0x3c5785f6, 0x3f027211, 0x3f394f43, 0x3f36ac22,
            0x3e0a5b0c,
        ],
    ),
];

#[test]
fn mt19937_matches_the_reference_stream() {
    for (seed, want) in RAW {
        let mut g = Mt19937::new(seed);
        let got: Vec<u32> = (0..8).map(|_| g.next()).collect();
        assert_eq!(got, want.to_vec(), "seed {seed}");
    }
}

/// ⚠️ Compared as BIT PATTERNS. Two floats that print the same can differ in the last place, and
/// the last place is what an annealer's accept/reject test turns on.
#[test]
fn canonical_floats_match_the_reference_stream() {
    for (seed, want) in FLOATS {
        let mut g = Mt19937::new(seed);
        let got: Vec<u32> = (0..8).map(|_| canonical_f32(&mut g).to_bits()).collect();
        assert_eq!(got, want.to_vec(), "seed {seed}");
    }
}

/// 🔑 **One draw per float, measured.** The probe took a single float and then read the generator
/// directly; what came out was the reference stream's SECOND word. This is the property that
/// keeps our stream aligned with upstream's once integer draws are interleaved on the same
/// generator — a value-correct implementation that consumed two words would still diverge.
#[test]
fn one_float_consumes_exactly_one_generator_word() {
    for ((seed, raw), _) in RAW.iter().zip(FLOATS.iter()) {
        let mut g = Mt19937::new(*seed);
        let _ = canonical_f32(&mut g);
        assert_eq!(g.next(), raw[1], "seed {seed}");
    }
}

/// ⚠️ The distribution is half-open: 1.0 must never come out. The word that forces it is the one
/// that rounds up to `2^32` in `f32`, which is any word above `0xffffff7f`.
#[test]
fn the_value_stays_below_one() {
    let mut g = Mt19937::new(0);
    for _ in 0..20_000 {
        let v = canonical_f32(&mut g);
        assert!((0.0..1.0).contains(&v), "{v} left the half-open range");
    }
}

// ---------------------------------------------------------------- the integer distribution

use vyges_mpl::rng::uniform_int;

/// `(seed, n, first twelve draws)` — measured against Boost **1.89.0**, the version the reference
/// build compiles against. ⚠️ Not the distribution shipped by any Ubuntu release; the build
/// installs 1.89 to `/usr/local` from source, so probing a distro package probes the wrong code.
const INTS: [(u32, u32, [u32; 12]); 8] = [
    (0, 2, [1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 0]),
    (0, 3, [1, 1, 2, 2, 1, 2, 1, 2, 1, 1, 1, 1]),
    (0, 100, [54, 59, 71, 84, 60, 85, 54, 84, 42, 62, 64, 38]),
    (0, 1000, [548, 592, 715, 844, 602, 857, 544, 847, 423, 623, 645, 384]),
    (42, 8, [2, 6, 7, 1, 5, 6, 4, 4, 1, 3, 1, 0]),
    (42, 17, [6, 13, 16, 3, 12, 13, 10, 10, 2, 7, 2, 1]),
    (42, 65535, [24545, 52201, 62305, 12021, 47971, 51097, 39233, 39114, 10224, 29217, 10223, 6551]),
    (42, 65537, [24546, 52203, 62306, 12021, 47972, 51098, 39234, 39115, 10224, 29218, 10223, 6552]),
];

#[test]
fn uniform_int_matches_the_reference_vectors() {
    for (seed, n, want) in INTS {
        let mut g = Mt19937::new(seed);
        let got: Vec<u32> = (0..12).map(|_| uniform_int(&mut g, n)).collect();
        assert_eq!(got, want.to_vec(), "seed {seed}, n {n}");
    }
}

/// ⛔ **The formula is NOT `(u * n) >> 32`.** That is the obvious reading and it is wrong: it
/// agrees on most inputs and disagrees by one, upward, on a few. These are measured
/// disagreements — the reference returns the first value, multiply-shift returns the second.
#[test]
fn the_bucket_division_is_not_a_multiply_shift() {
    for (u, n, reference, multiply_shift) in
        [(3_371_549_311u32, 1000u32, 785u32, 784u32),
         (1_731_553_077, 65535, 26421, 26420),
         (83_360_520, 65537, 1272, 1271)]
    {
        assert_eq!(((u as u64) * n as u64 >> 32) as u32, multiply_shift, "the wrong formula");
        // The bucket division, spelled out, is what the implementation does.
        let range = (n - 1) as u64;
        let mut bucket = u32::MAX as u64 / (range + 1);
        if u32::MAX as u64 % (range + 1) == range {
            bucket += 1;
        }
        assert_eq!((u as u64 / bucket) as u32, reference, "u={u}, n={n}");
        assert_ne!(reference, multiply_shift, "this case must actually distinguish them");
    }
}

/// 🔑 **Five million draws, folded.** The vectors above would still pass if a rejection were
/// mishandled — rejection fires roughly once in `2^32 / n` draws, so a twelve-draw vector cannot
/// see one. A checksum over the whole stream can: one extra or missing generator word shifts
/// every later value.
#[test]
fn uniform_int_matches_the_reference_over_five_million_draws() {
    const SUMS: [(u32, u64); 6] = [
        (2, 1670541306744283576),
        (3, 18354083997637997716),
        (100, 648388948599899703),
        (1000, 1388673509648223755),
        (65535, 8176314730070891872),
        (65537, 667375015722121527),
    ];
    for (n, want) in SUMS {
        let mut g = Mt19937::new(999);
        let mut sum: u64 = 1469598103934665603;
        for _ in 0..5_000_000 {
            sum ^= uniform_int(&mut g, n) as u64;
            sum = sum.wrapping_mul(1099511628211);
        }
        assert_eq!(sum, want, "n {n}");
    }
}
