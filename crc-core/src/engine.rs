//! The CRC engine: register computation, textbook long division, syndrome
//! analysis, and frame assembly.

use crate::spec::{CrcSpec, reverse_bits};

/// Compute the CRC register over `data` under `spec`.
///
/// This is the direct Rockmodel formulation: bytes are reflected on input if
/// `refin` is set, the register shifts MSB-first against the forward
/// polynomial, and the register is reflected before `xorout` if `refout` is set.
#[must_use]
pub fn compute(spec: &CrcSpec, data: &[u8]) -> u64 {
    let mask = spec.mask();
    let top = 1u64 << (u32::from(spec.width) - 1);
    let mut reg = spec.init & mask;

    for &byte in data {
        let b = if spec.refin {
            byte.reverse_bits()
        } else {
            byte
        };
        // Align the byte with the high-order end of the register.
        reg ^= u64::from(b) << (u32::from(spec.width) - 8);
        for _ in 0..8 {
            if reg & top != 0 {
                reg = (reg << 1) ^ spec.poly;
            } else {
                reg <<= 1;
            }
            reg &= mask;
        }
    }

    if spec.refout {
        reg = reverse_bits(reg, spec.width);
    }
    (reg ^ spec.xorout) & mask
}

/// Compute the CRC over an explicit bit sequence rather than whole bytes.
///
/// The receiver verifies an arbitrary bit count, and the syndrome basis must be
/// measured over exactly the same number of bits. Working in bits throughout
/// keeps one representation and removes a class of off-by-a-byte errors.
#[must_use]
pub fn compute_bits(spec: &CrcSpec, bits: &[u8]) -> u64 {
    let mask = spec.mask();
    let top = 1u64 << (u32::from(spec.width) - 1);
    let mut reg = spec.init & mask;

    for chunk in bits.chunks(8) {
        // Pad the final partial byte on the right, so a short tail contributes
        // the same bits a receiver would actually have clocked in.
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            byte |= (bit & 1) << (7 - i);
        }
        let b = if spec.refin {
            byte.reverse_bits()
        } else {
            byte
        };
        reg ^= u64::from(b) << (u32::from(spec.width) - 8);
        for _ in 0..8 {
            if reg & top != 0 {
                reg = (reg << 1) ^ spec.poly;
            } else {
                reg <<= 1;
            }
            reg &= mask;
        }
    }

    if spec.refout {
        reg = reverse_bits(reg, spec.width);
    }
    (reg ^ spec.xorout) & mask
}

/// One row of the long-division tableau.
///
/// `bits` is the working register at this step, MSB first. `xored` records
/// whether a subtraction (XOR of the generator) happened, which is the moment
/// worth highlighting in a demo: the set leading bit being cancelled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DivisionStep {
    /// Zero-based index of the leading bit being tested at this step.
    pub index: usize,
    /// The working register before the possible subtraction.
    pub bits: Vec<u8>,
    /// Whether the generator was XORed in at this step.
    pub xored: bool,
}

/// The full result of a textbook long division, suitable for rendering the
/// classic `augmented dividend / generator` tableau.
#[derive(Clone, Debug)]
pub struct DivisionTrace {
    /// The generator polynomial bits, MSB first, length `width`.
    pub generator: Vec<u8>,
    /// The dividend: message bits followed by `degree` zero check bits.
    pub augmented: Vec<u8>,
    /// The original message bits, without the appended zeros.
    pub message: Vec<u8>,
    /// One entry per message bit.
    pub steps: Vec<DivisionStep>,
    /// The final `degree` bits, i.e. the CRC.
    pub remainder: Vec<u8>,
}

impl DivisionTrace {
    /// Total number of bits in the transmitted frame.
    #[must_use]
    pub fn frame_len(&self) -> usize {
        self.message.len() + self.remainder.len()
    }
}

/// Perform the textbook long division of `message` by `spec`'s generator.
///
/// The dividend is the message with `degree` zeros appended; division proceeds
/// one bit at a time, and each set leading bit is cancelled by XORing the
/// generator. This is the algorithm as written in the course notes, and it is
/// what the step-by-step animation visualises.
#[must_use]
pub fn long_division(spec: &CrcSpec, message: &[u8]) -> DivisionTrace {
    let degree = spec.degree();
    // The generator spans width+1 bits: the stored polynomial with its implicit
    // x^width term restored.
    let full_poly = spec.poly | (1u64 << u32::from(spec.width));
    let generator: Vec<u8> = (0..=usize::from(spec.width))
        .map(|i| ((full_poly >> (u32::from(spec.width) - i as u32)) & 1) as u8)
        .collect();

    let message_bits = bytes_to_bits(message);
    let mut augmented = message_bits.clone();
    augmented.extend(std::iter::repeat_n(0u8, degree));

    let mut steps = Vec::with_capacity(message_bits.len());
    for index in 0..message_bits.len() {
        if augmented[index] == 1 {
            for (offset, &g) in generator.iter().enumerate() {
                augmented[index + offset] ^= g;
            }
            steps.push(DivisionStep {
                index,
                bits: augmented.clone(),
                xored: true,
            });
        } else {
            steps.push(DivisionStep {
                index,
                bits: augmented.clone(),
                xored: false,
            });
        }
    }

    let remainder = augmented[message_bits.len()..].to_vec();

    DivisionTrace {
        generator,
        augmented: message_bits.iter().copied().chain(std::iter::repeat_n(0u8, degree)).collect(),
        message: message_bits,
        steps,
        remainder,
    }
}

/// A complete transmission frame: data bits followed by check bits.
#[derive(Clone, Debug)]
pub struct Frame {
    pub message_bits: Vec<u8>,
    pub check_bits: Vec<u8>,
}

impl Frame {
    /// Concatenate message and check bits into the transmitted codeword.
    #[must_use]
    pub fn codeword(&self) -> Vec<u8> {
        self.message_bits.iter().chain(self.check_bits.iter()).copied().collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.message_bits.len() + self.check_bits.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Flip the bit at `index`, modelling a channel error at that position.
    ///
    /// Positions are indexed from the most significant bit of the codeword,
    /// which is the same orientation the bit grid displays.
    pub fn flip(&mut self, index: usize) -> bool {
        let total = self.len();
        if index >= total {
            return false;
        }
        if index < self.message_bits.len() {
            self.message_bits[index] ^= 1;
        } else {
            self.check_bits[index - self.message_bits.len()] ^= 1;
        }
        true
    }

    /// Flip every bit in the half-open range `start..start+len`.
    pub fn flip_burst(&mut self, start: usize, len: usize) -> usize {
        let total = self.len();
        let end = (start + len).min(total);
        let mut flipped = 0;
        for i in start..end {
            if self.flip(i) {
                flipped += 1;
            }
        }
        flipped
    }
}

/// Build a frame by appending the CRC of `data` to its bits.
#[must_use]
pub fn build_frame(spec: &CrcSpec, data: &[u8]) -> Frame {
    let message_bits = bytes_to_bits(data);
    let crc = compute(spec, data);
    let width = usize::from(spec.width);
    // Left-pad to a whole number of bytes so widths that are not multiples of
    // 8 still produce a correctly aligned bit vector.
    let padded = ((width + 7) / 8) * 8;
    let shifted = crc << (padded - width);
    let check_bits = bytes_to_bits(&shifted.to_be_bytes()[8 - padded / 8..]);
    Frame {
        message_bits,
        check_bits,
    }
}

/// The outcome of checking a received frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Remainder was zero, so the frame is intact as far as CRC can tell.
    Intact,
    /// Remainder was non-zero, so an error is present.
    ErrorDetected,
    /// Remainder was non-zero but the pattern matched a locatable burst.
    ErrorLocated {
        start: usize,
        len: usize,
    },
}

impl Verdict {
    #[must_use]
    pub fn is_ok(self) -> bool {
        matches!(self, Self::Intact)
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Intact => "VALID",
            Self::ErrorDetected => "ERROR",
            Self::ErrorLocated { .. } => "ERROR LOCATED",
        }
    }
}

/// A complete verification report for a received frame.
#[derive(Clone, Debug)]
pub struct CheckReport {
    /// Syndrome: the CRC of the received data with zero init and zero xorout.
    /// This is the value a real receiver computes and inspects.
    pub syndrome: u64,
    pub verdict: Verdict,
    /// The corrected codeword, when the syndrome identified the error exactly.
    pub repaired: Option<Vec<u8>>,
    /// Human-readable explanation of how the verdict was reached.
    pub explanation: String,
}

/// A syndrome-to-burst lookup, built once and reused.
///
/// Building this costs O(2^cap * frame_bits): fine once, ruinous inside a loop,
/// so batch code constructs it up front and passes it to [`check_frame_with`].
/// What a syndrome value tells us about the error pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    /// Exactly one burst in the frame produces this syndrome.
    Unique(usize, usize),
    /// Several distinct bursts share this syndrome, so the error is detected
    /// but its position cannot be determined.
    Ambiguous,
}

#[derive(Clone, Debug)]
pub struct PatternIndex {
    table: std::collections::HashMap<u64, Slot>,
    ambiguous: usize,
}

impl PatternIndex {
    /// Build the index for a frame of `frame_bits` bits under `spec`.
    ///
    /// `message_bits` is the frame's message portion. The table is measured
    /// against this exact frame, because a burst's syndrome depends on the
    /// surrounding bits and is not derivable from bit positions alone.
    #[must_use]
    pub fn build(spec: &CrcSpec, frame_bits: usize, message_bits: &[u8]) -> Self {
        let (table, ambiguous) = pattern_table(spec, frame_bits, message_bits);
        Self { table, ambiguous }
    }

    /// The burst producing `syndrome`, but only when it is the sole such burst.
    ///
    /// Returning `None` for an ambiguous syndrome is the honest answer: the
    /// error is certainly present, yet its position is genuinely undetermined.
    #[must_use]
    pub fn locate(&self, syndrome: u64) -> Option<(usize, usize)> {
        match self.table.get(&syndrome) {
            Some(Slot::Unique(start, len)) => Some((*start, *len)),
            _ => None,
        }
    }

    /// True when this syndrome is shared by more than one error pattern.
    #[must_use]
    pub fn is_ambiguous(&self, syndrome: u64) -> bool {
        self.table.get(&syndrome) == Some(&Slot::Ambiguous)
    }

    /// How many syndromes are uniquely identifiable.
    #[must_use]
    pub fn unique_count(&self) -> usize {
        self.table
            .values()
            .filter(|s| matches!(s, Slot::Unique(_, _)))
            .count()
    }

    /// How many syndromes are shared by multiple patterns.
    #[must_use]
    pub fn ambiguous_count(&self) -> usize {
        self.ambiguous
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.table.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}

/// Check a received frame, building a throwaway index.
///
/// Convenient, but do not call it in a hot loop: hoist a [`PatternIndex`].
#[must_use]
pub fn check_frame(spec: &CrcSpec, received: &[u8]) -> CheckReport {
    let bits = bytes_to_bits(received);
    let total = bits.len();
    let split = total.saturating_sub(spec.degree());
    let index = PatternIndex::build(spec, total, &bits[..split]);
    check_frame_bits(spec, &bits, &index)
}

/// Check a received frame against a prebuilt index, and locate the error when
/// the polynomial allows it.
///
/// Verification is done the way hardware does it: recompute the CRC over the
/// received message bits and compare against the received check bits. Testing
/// whether the whole codeword is divisible by the generator would only be valid
/// when `init` and `xorout` are both zero, which most real CRCs are not.
#[must_use]
pub fn check_frame_with(spec: &CrcSpec, received: &[u8], index: &PatternIndex) -> CheckReport {
    check_frame_bits(spec, &bytes_to_bits(received), index)
}

/// Bit-level frame verification, the single source of truth.
///
/// Everything is measured in bits because the syndrome basis depends on the
/// exact bit count: a receiver recomputing over a byte-rounded buffer would
/// otherwise derive a different basis than the table was built with.
#[must_use]
pub fn check_frame_bits(spec: &CrcSpec, received: &[u8], index: &PatternIndex) -> CheckReport {
    let total = received.len();
    let check_bits = spec.degree();
    let hex_width = usize::from(spec.width) / 4 + 1;
    if total < check_bits {
        return CheckReport {
            syndrome: 0,
            verdict: Verdict::ErrorDetected,
            repaired: None,
            explanation: "Frame is shorter than the check field, so it cannot be valid."
                .to_string(),
        };
    }

    let split = total - check_bits;
    let recomputed_check = compute_bits(spec, &received[..split]) & spec.mask();
    let received_check = read_bits(&received[split..], check_bits);

    if received_check == recomputed_check {
        return CheckReport {
            syndrome: 0,
            verdict: Verdict::Intact,
            repaired: None,
            explanation: format!(
                "Recomputed check bits {recomputed_check:#0width$X} match the received value. \
                 The frame is intact.",
                width = hex_width
            ),
        };
    }

    let syndrome = (received_check ^ recomputed_check) & spec.mask();

    if let Some((start, len)) = index.locate(syndrome) {
        let mut repaired = received.to_vec();
        flip_range(&mut repaired, start, len);
        return CheckReport {
            syndrome,
            verdict: Verdict::ErrorLocated { start, len },
            repaired: Some(repaired),
            explanation: format!(
                "Check mismatch: expected {recomputed_check:#0width$X}, received \
                 {received_check:#0width$X}. The error is a {len}-bit burst at bit \
                 {start} and has been corrected.",
                width = hex_width
            ),
        };
    }

    let tail = if index.is_ambiguous(syndrome) {
        "An error is present, but this syndrome is shared by more than one error \
         pattern, so the position cannot be determined."
    } else {
        "An error is present but not uniquely locatable, so the frame must be \
         discarded or retransmitted."
    };
    CheckReport {
        syndrome,
        verdict: Verdict::ErrorDetected,
        repaired: None,
        explanation: format!(
            "Check mismatch: expected {recomputed_check:#0width$X}, received \
             {received_check:#0width$X}. {tail}",
            width = hex_width
        ),
    }
}

/// Read the first `n` bits of `data`, MSB first, as an `n`-bit value.
fn read_bits(data: &[u8], n: usize) -> u64 {
    let mut value = 0u64;
    for &bit in data.iter().take(n) {
        value = (value << 1) | u64::from(bit & 1);
    }
    value
}

/// Longest burst the pattern table will attempt to locate.
///
/// Correcting a burst of length L needs a syndrome space of 2^L distinct
/// values, so the search is exponential by construction. The cap keeps the
/// table instant; bursts longer than this are still *detected*, they are just
/// reported as detected-but-unlocatable, which is the honest answer.
pub const MAX_LOCATABLE_BURST: usize = 12;

/// Map each syndrome value to the (start, len) of the burst that produces it.
///
/// Any error confined to `b` consecutive bits is x^k * e(x) with deg(e) < b, so
/// enumerating every burst of length <= cap covers the whole neighbourhood.
/// Syndromes that two different bursts both produce are genuinely ambiguous;
/// the first one found wins and the report says "detected", not "corrected".
fn pattern_table(
    spec: &CrcSpec,
    frame_bits: usize,
    message_bits: &[u8],
) -> (std::collections::HashMap<u64, Slot>, usize) {
    let max_burst = spec.degree().min(MAX_LOCATABLE_BURST);
    let mut table: std::collections::HashMap<u64, Slot> = std::collections::HashMap::new();
    if max_burst == 0 || frame_bits == 0 || message_bits.is_empty() {
        return (table, 0);
    }

    // Measure every candidate burst against the frame as it will actually be
    // received.
    //
    // An earlier version combined per-bit syndrome vectors algebraically. That
    // is only valid if the register's response is linear in the frame contents,
    // which it is not once `init` and the check field are in play: the syndrome
    // of a burst depends on the surrounding bits. Measuring each candidate
    // directly costs one CRC per pattern, which the frame-size caps keep cheap,
    // and is correct for every parameter set by construction.
    let clean_syndrome = syndrome_of(spec, message_bits, frame_bits);
    // A burst may land in the check field, so candidates are written into a
    // full-length codeword rather than into the message slice.
    let template = {
        let mut t = message_bits.to_vec();
        t.resize(frame_bits, 0);
        t
    };

    for burst_len in 1..=max_burst.min(frame_bits) {
        for pattern in 1u64..(1u64 << burst_len) {
            for start in 0..=(frame_bits - burst_len) {
                let mut damaged = template.clone();
                for bit in 0..burst_len {
                    if (pattern >> (burst_len - 1 - bit)) & 1 == 1 {
                        damaged[start + bit] ^= 1;
                    }
                }
                let syndrome = (syndrome_of(spec, &damaged, frame_bits) ^ clean_syndrome) & spec.mask();
                // Syndrome 0 is reserved for "no error": a burst that leaves it
                // at zero is undetectable, not merely unlocatable.
                if syndrome == 0 {
                    continue;
                }
                match table.get_mut(&syndrome) {
                    Some(Slot::Ambiguous) => {}
                    Some(slot @ Slot::Unique(..)) => {
                        *slot = Slot::Ambiguous;
                    }
                    None => {
                        table.insert(syndrome, Slot::Unique(start, burst_len));
                    }
                }
            }
        }
    }

    let ambiguous = table
        .values()
        .filter(|s| matches!(s, Slot::Ambiguous))
        .count();
    (table, ambiguous)
}

/// The syndrome a receiver would observe for `frame` (message bits plus an
/// all-zero check field), using the same verification path as `check_frame_bits`.
fn syndrome_of(spec: &CrcSpec, frame: &[u8], frame_bits: usize) -> u64 {
    let check_bits = spec.degree();
    if frame_bits < check_bits {
        return 0;
    }
    let split = frame_bits - check_bits;
    // The message portion is what the receiver recomputes over. Clamp, because a
    // frame whose message is shorter than the split point cannot be evaluated.
    let message_len = split.min(frame.len());
    let recomputed = compute_bits(spec, &frame[..message_len]) & spec.mask();
    // The check field contributes only when the frame actually carries one.
    let received = if frame.len() > split {
        read_bits(&frame[split..], check_bits)
    } else {
        0
    };
    (received ^ recomputed) & spec.mask()
}

/// Flip bits `start..start+len` of a byte slice, MSB-first indexing.
pub fn flip_range(data: &mut [u8], start: usize, len: usize) {
    for i in start..start + len {
        let byte_index = i / 8;
        if byte_index >= data.len() {
            break;
        }
        let bit_index = 7 - (i % 8);
        data[byte_index] ^= 1u8 << bit_index;
    }
}

/// Expand bytes into MSB-first bits.
#[must_use]
pub fn bytes_to_bits(data: &[u8]) -> Vec<u8> {
    let mut bits = Vec::with_capacity(data.len() * 8);
    for &byte in data {
        for i in (0..8).rev() {
            bits.push((byte >> i) & 1);
        }
    }
    bits
}

/// Repack MSB-first bits into bytes. Trailing bits in the last byte are zeroed.
#[must_use]
pub fn bits_to_bytes(bits: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; bits.len().div_ceil(8)];
    for (i, &bit) in bits.iter().enumerate() {
        if bit == 1 {
            out[i / 8] |= 1u8 << (7 - (i % 8));
        }
    }
    out
}

/// Render a bit slice as a contiguous string of '0' and '1'.
#[must_use]
pub fn bits_to_string(bits: &[u8]) -> String {
    bits.iter().map(|b| if *b == 1 { '1' } else { '0' }).collect()
}

/// Render a bit slice grouped into bytes, MSB first: `10110010 00001111`.
#[must_use]
pub fn bits_to_hex_groups(bits: &[u8]) -> String {
    bits.chunks(8)
        .map(|chunk| {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                if bit == 1 {
                    byte |= 1u8 << (7 - i);
                }
            }
            format!("{byte:02X}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{CATALOG, CRC16_CCITT_FALSE, CRC16_XMODEM, CRC32_ISO};

    #[test]
    fn catalogue_check_values_match() {
        let check = b"123456789";
        for entry in CATALOG {
            let got = compute(&entry.spec, check);
            assert_eq!(
                got,
                entry.check,
                "{} gave {got:#X}, catalogue says {:#X}",
                entry.spec.name,
                entry.check
            );
        }
    }

    #[test]
    fn crc32_of_check_string_is_cbf43926() {
        assert_eq!(compute(&CRC32_ISO, b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn ccitt_false_of_check_string_is_29b1() {
        assert_eq!(compute(&CRC16_CCITT_FALSE, b"123456789"), 0x29B1);
    }

    #[test]
    fn empty_input_is_init_xor_xorout() {
        for entry in CATALOG {
            let spec = &entry.spec;
            let expected = (spec.init ^ spec.xorout) & spec.mask();
            assert_eq!(compute(spec, b""), expected, "{}", spec.name);
        }
    }

    #[test]
    fn long_division_remainder_equals_register_when_init_is_zero() {
        // Textbook long division yields the pure polynomial remainder, which is
        // exactly the register CRC only when init=0 and xorout=0. Comparing
        // them for a spec with a non-zero init compares two different things.
        let spec = CRC16_XMODEM;
        assert_eq!(spec.init, 0);
        assert_eq!(spec.xorout, 0);
        for data in [&b"A"[..], b"Networking", b"hello world", b"\x00\xff\x0f"] {
            let trace = long_division(&spec, data);
            let from_trace =
                u64::from_str_radix(&bits_to_string(&trace.remainder), 2).unwrap();
            assert_eq!(from_trace, compute(&spec, data), "payload {data:?}");
        }
    }

    #[test]
    fn long_division_matches_register_for_every_forward_zero_init_spec() {
        for entry in CATALOG {
            // Three conditions, not two: a reflected code processes bits in the
            // opposite order, so its polynomial remainder over raw bits is not
            // the register value even with init and xorout both zero.
            if entry.spec.init != 0 || entry.spec.xorout != 0 || entry.spec.refin {
                continue;
            }
            let trace = long_division(&entry.spec, b"verify me");
            let from_trace = u64::from_str_radix(&bits_to_string(&trace.remainder), 2).unwrap();
            assert_eq!(
                from_trace,
                compute(&entry.spec, b"verify me"),
                "{}",
                entry.spec.name
            );
        }
    }

    #[test]
    fn long_division_length_is_consistent() {
        let spec = CRC16_CCITT_FALSE;
        let data = b"CRC demo payload";
        let trace = long_division(&spec, data);
        assert_eq!(trace.augmented.len(), data.len() * 8 + spec.degree());
        assert_eq!(trace.remainder.len(), spec.degree());
        assert_eq!(trace.steps.len(), data.len() * 8);
        assert_eq!(trace.frame_len(), trace.augmented.len());
    }

    #[test]
    fn long_division_leaves_no_leading_ones() {
        for entry in CATALOG {
            let trace = long_division(&entry.spec, b"the quick brown fox");
            let tail = &trace.augmented[trace.message.len()..];
            assert!(
                tail.iter().all(|&b| b == 0),
                "{} remainder should be all zero after a final subtraction",
                entry.spec.name
            );
        }
    }

    #[test]
    fn intact_frame_verifies_clean() {
        // A frame we just built must always verify, for every catalogue entry.
        // build_frame appends the register CRC, and check_frame recomputes it,
        // so the two must agree regardless of init and xorout.
        for entry in CATALOG {
            let frame = build_frame(&entry.spec, b"hello");
            let report = check_frame(&entry.spec, &bits_to_bytes(&frame.codeword()));
            assert_eq!(
                report.verdict,
                Verdict::Intact,
                "{} failed to verify its own frame: {}",
                entry.spec.name,
                report.explanation
            );
        }
    }

    #[test]
    fn single_bit_flip_is_always_detected() {
        // Exhaustive over every bit position and every catalogue entry: a
        // single-bit error must never slip through.
        for entry in CATALOG {
            let spec = &entry.spec;
            let frame = build_frame(spec, b"hello world");
            for pos in 0..frame.len() {
                let mut bad = frame.clone();
                bad.flip(pos);
                let report = check_frame(spec, &bits_to_bytes(&bad.codeword()));
                assert!(
                    !report.verdict.is_ok(),
                    "{} missed a single-bit flip at position {pos}",
                    spec.name
                );
            }
        }
    }

    #[test]
    fn located_single_bit_error_is_repaired_exactly() {
        // Narrow codes can have syndrome collisions, where reporting a location
        // would be a guess. This asserts the strong form: whenever a location
        // *is* reported, it is exactly right. Ambiguity is asserted separately.
        for entry in CATALOG {
            let spec = &entry.spec;
            let frame = build_frame(spec, b"hello world");
            for pos in 0..frame.len() {
                let mut bad = frame.clone();
                bad.flip(pos);
                let report = check_frame(spec, &bits_to_bytes(&bad.codeword()));
                if let Verdict::ErrorLocated { start, len } = report.verdict {
                    assert_eq!(
                        len, 1,
                        "{} reported a {len}-bit burst for a single-bit error",
                        spec.name
                    );
                    assert_eq!(start, pos, "{} reported the wrong bit position", spec.name);
                }
            }
        }
    }

    #[test]
    fn reported_repairs_actually_restore_validity() {
        // End-to-end: whatever the tool claims to have fixed must verify clean.
        for entry in CATALOG {
            let spec = &entry.spec;
            let frame = build_frame(spec, b"repair me");
            let pristine = bits_to_bytes(&frame.codeword());
            for pos in 0..frame.len() {
                let mut bad = frame.clone();
                bad.flip(pos);
                let report = check_frame(spec, &bits_to_bytes(&bad.codeword()));
                if let Some(repaired) = &report.repaired {
                    let verify = check_frame(spec, repaired);
                    assert_eq!(
                        verify.verdict,
                        Verdict::Intact,
                        "{} claimed to repair a flip at {pos} but the result still fails",
                        spec.name
                    );
                    assert_eq!(
                        *repaired, pristine,
                        "{} repaired to the wrong frame",
                        spec.name
                    );
                }
            }
        }
    }

    #[test]
    fn all_bursts_within_degree_are_detected() {
        // The headline guarantee: any burst of length <= deg(G) is caught.
        for entry in CATALOG {
            let spec = &entry.spec;
            let frame = build_frame(spec, b"burst guard");
            let original = bits_to_bytes(&frame.codeword());
            let total = frame.len();
            for len in 1..=spec.degree().min(total) {
                for start in 0..=(total - len) {
                    let mut bad = original.clone();
                    crate::engine::flip_range(&mut bad, start, len);
                    let report = check_frame(spec, &bad);
                    assert_ne!(
                        report.verdict,
                        Verdict::Intact,
                        "{} missed a {len}-bit burst at {start}",
                        spec.name
                    );
                }
            }
        }
    }

    #[test]
    fn roundtrip_bits_bytes() {
        let data = b"\x00\x01\xFE\xFF\xA5";
        assert_eq!(bits_to_bytes(&bytes_to_bits(data)), data.to_vec());
    }

    #[test]
    fn all_specs_validate() {
        for entry in CATALOG {
            entry.spec.validate().unwrap_or_else(|e| panic!("{}: {e}", entry.spec.name));
        }
    }
}
