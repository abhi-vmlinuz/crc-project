//! Parameterised CRC engine.
//!
//! The public surface is deliberately small: pick a [`CrcSpec`], compute a
//! checksum, build a frame, and check what came back. Everything else exists to
//! make the algorithm visible rather than to be called in a hot loop.

pub mod engine;
pub mod montecarlo;
pub mod spec;

pub use engine::{
    CheckReport, DivisionStep, DivisionTrace, Frame, MAX_LOCATABLE_BURST, PatternIndex, Verdict,
    bits_to_bytes, bits_to_hex_groups, bits_to_string, build_frame, bytes_to_bits, check_frame,
    check_frame_bits, check_frame_with, compute, compute_bits, long_division,
};
pub use montecarlo::{BurstResult, Rng, run_burst_batch, sweep};
pub use spec::{
    CATALOG, CRC8, CRC8_HITAG, CRC8_MAXIM, CRC8_SMBUS, CRC16_ARC, CRC16_CCITT_FALSE, CRC16_IBM,
    CRC16_MODBUS, CRC16_XMODEM, CRC32_BZIP2, CRC32_ISO, CRC32_MPEG2, CRC32_POSIX, CRC64_XZ,
    CatalogEntry, CrcSpec, SpecError, find_by_name, reverse_bits, reverse_byte,
};
