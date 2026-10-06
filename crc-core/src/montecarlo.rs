//! Monte-Carlo characterisation of CRC error detection.
//!
//! The theoretical result being tested: a CRC of width `w` with generator
//! `G` guarantees detection of all burst errors of length `L` provided
//! `L <= degree(G)`, and the probability of *failing* to detect a random error
//! pattern of that length is on the order of `2^-(degree - L)`. The simulation
//! should track that curve. If it does not, the implementation is wrong.

use crate::engine::{PatternIndex, bits_to_bytes, bytes_to_bits, compute};
use crate::spec::CrcSpec;

/// Deterministic PRNG so results are reproducible for a given seed, which
/// matters when the same numbers are quoted in a report.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// xorshift64*, small and adequate for injecting random error patterns.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[0, n)`. Rejection sampling avoids modulo bias.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        let zone = u64::MAX - (u64::MAX % n as u64);
        loop {
            let v = self.next_u64();
            if v < zone {
                return (v % n as u64) as usize;
            }
        }
    }
}

/// Outcome of one batch of trials at a given burst length.
#[derive(Clone, Debug)]
pub struct BurstResult {
    pub burst_len: usize,
    pub trials: u32,
    /// Errors that the CRC failed to notice. Should be zero for `burst_len <= degree`.
    pub undetected: u32,
    /// Errors detected, whether or not locatable.
    pub detected: u32,
    /// Detected *and* the pattern uniquely identified.
    pub located: u32,
    /// Theoretical undetected rate, `2^-(degree - L)` for `L <= degree`.
    pub theoretical: f64,
}

impl BurstResult {
    #[must_use]
    pub fn observed_rate(&self) -> f64 {
        if self.trials == 0 {
            0.0
        } else {
            f64::from(self.undetected) / f64::from(self.trials)
        }
    }

    #[must_use]
    pub fn detection_rate(&self) -> f64 {
        if self.trials == 0 {
            1.0
        } else {
            f64::from(self.detected) / f64::from(self.trials)
        }
    }
}

/// Run one batch of trials injecting bursts of `burst_len` bits.
///
/// Returns zero undetected whenever `burst_len <= degree`, which is the
/// guarantee the theory makes. Any non-zero count there is a genuine bug.
#[must_use]
pub fn run_burst_batch(
    spec: &CrcSpec,
    payload: &[u8],
    burst_len: usize,
    trials: u32,
    seed: u64,
) -> BurstResult {
    let degree = spec.degree();
    let message_bits = bytes_to_bits(payload);
    let frame_bits = message_bits.len() + degree;
    let mut rng = Rng::new(seed);

    // Built once for the whole batch. Constructing it per trial dominates the
    // runtime and would make the sweep pointless.
    let index = PatternIndex::build(spec, frame_bits, &message_bits);

    let mut undetected = 0u32;
    let mut detected = 0u32;
    let mut located = 0u32;

    for _ in 0..trials {
        let mut bits = message_bits.clone();
        bits.extend(std::iter::repeat_n(0u8, degree));

        if burst_len == 0 {
            // Zero-length burst: the control case, must always be clean.
            let ok = syndrome_is_zero(spec, &bits);
            if !ok {
                detected += 1;
            }
            continue;
        }

        let effective = burst_len.min(frame_bits);
        let start = rng.below(frame_bits - effective + 1);
        for i in 0..effective {
            let bit = (rng.next_u64() & 1) as u8;
            bits[start + i] ^= bit;
        }

        if syndrome_is_zero(spec, &bits) {
            // The error changed the polynomial by a multiple of G, so the
            // receiver cannot tell this frame from a valid one.
            undetected += 1;
        } else {
            detected += 1;
            if crate::engine::check_frame_with(spec, &bits_to_bytes(&bits), &index)
                .repaired
                .is_some()
            {
                located += 1;
            }
        }
    }

    let theoretical = if burst_len > degree {
        // Beyond the guarantee; a random error is essentially never missed.
        0.0
    } else {
        2f64.powi(-((degree - burst_len) as i32))
    };

    BurstResult {
        burst_len,
        trials,
        undetected,
        detected,
        located,
        theoretical,
    }
}

/// True when the frame's polynomial is divisible by the generator.
#[must_use]
pub fn syndrome_is_zero(spec: &CrcSpec, bits: &[u8]) -> bool {
    let mut probe = *spec;
    probe.init = 0;
    probe.xorout = 0;
    compute(&probe, &bits_to_bytes(bits)) == 0
}

/// Sweep burst lengths from 1 up to `max_len`, one batch each.
#[must_use]
pub fn sweep(
    spec: &CrcSpec,
    payload: &[u8],
    max_len: usize,
    trials: u32,
    seed: u64,
) -> Vec<BurstResult> {
    (1..=max_len)
        .map(|len| run_burst_batch(spec, payload, len, trials, seed ^ (len as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{CRC16_CCITT_FALSE, CRC32_ISO};

    #[test]
    fn no_undetected_errors_within_guarantee() {
        // The central claim: every burst up to degree(G) is caught.
        for spec in [CRC16_CCITT_FALSE, CRC32_ISO] {
            for len in 1..=spec.degree() {
                let result = run_burst_batch(&spec, b"monte carlo payload", len, 2000, 0xDEAD_BEEF);
                assert_eq!(
                    result.undetected, 0,
                    "{} missed an error in a {len}-bit burst, but degree is {}",
                    spec.name, spec.degree()
                );
                assert_eq!(result.detection_rate(), 1.0);
            }
        }
    }

    #[test]
    fn rng_is_reproducible() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn below_respects_bound() {
        let mut rng = Rng::new(7);
        for _ in 0..1000 {
            assert!(rng.below(17) < 17);
        }
    }
}
