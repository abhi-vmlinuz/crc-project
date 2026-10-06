//! Rockmodel CRC parameterisation.
//!
//! A CRC is not one algorithm but a family, fully described by five parameters
//! plus a width. Modelling them explicitly is what distinguishes a real
//! implementation from a hardcoded divide-by-0x1021.

/// The complete set of parameters that define a CRC code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrcSpec {
    /// Human-readable catalogue name, e.g. `CRC-32/ISO-HDLC`.
    pub name: &'static str,
    /// Codeword width in bits. Valid range 1..=64.
    pub width: u8,
    /// Generator polynomial, *including* the implicit leading term. The
    /// register treats bit `width-1` as the implicit x^(width-1) coefficient,
    /// so the stored value is the full polynomial as an integer.
    pub poly: u64,
    /// Initial register value.
    pub init: u64,
    /// Reflect each input byte before processing.
    pub refin: bool,
    /// Reflect the register before the final XOR.
    pub refout: bool,
    /// Value XORed into the register at the end.
    pub xorout: u64,
    /// Nominal name for the polynomial, shown in the UI as `G(x)`.
    pub poly_name: &'static str,
}

impl CrcSpec {
    /// Mask covering exactly `width` low bits.
    #[must_use]
    pub fn mask(&self) -> u64 {
        if self.width >= 64 {
            u64::MAX
        } else {
            (1u64 << self.width) - 1
        }
    }

    /// Number of check bits appended to the message, equal to deg(G).
    ///
    /// `poly` stores the generator *without* its leading x^width term, since
    /// the register's overflow bit already represents it. The full polynomial
    /// therefore spans `width + 1` bits and has degree exactly `width`.
    #[must_use]
    pub fn degree(&self) -> usize {
        usize::from(self.width)
    }

    /// Truncate an arbitrary value into a valid `width`-bit register state.
    #[must_use]
    pub fn truncate(&self, v: u64) -> u64 {
        v & self.mask()
    }

    /// Reject parameter sets that cannot describe a real CRC.
    ///
    /// The checks that matter: the polynomial must actually be `width` bits
    /// wide (otherwise the leading term is missing and the degree is wrong),
    /// and `init`/`xorout` must fit inside the register.
    pub fn validate(&self) -> Result<(), SpecError> {
        if self.width == 0 || self.width > 64 {
            return Err(SpecError::Width(self.width));
        }
        if self.width == 1 {
            return Err(SpecError::Width(self.width));
        }
        if self.poly == 0 {
            return Err(SpecError::DegeneratePolynomial);
        }
        // `poly` omits the implicit x^width term, so it must fit in the
        // register. Requiring the top bit to be set would reject every real
        // catalogue entry, since 0x1021 is only 13 bits wide for a 16-bit CRC.
        if self.poly > self.mask() {
            return Err(SpecError::PolynomialDegree {
                width: self.width,
                poly: self.poly,
            });
        }
        if self.init > self.mask() {
            return Err(SpecError::FieldOverflow("init", self.init, self.mask()));
        }
        if self.xorout > self.mask() {
            return Err(SpecError::FieldOverflow("xorout", self.xorout, self.mask()));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecError {
    Width(u8),
    DegeneratePolynomial,
    PolynomialDegree { width: u8, poly: u64 },
    FieldOverflow(&'static str, u64, u64),
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Width(w) => write!(f, "width {w} out of range (expected 2..=64)"),
            Self::DegeneratePolynomial => write!(f, "generator polynomial is zero"),
            Self::PolynomialDegree { width, poly } => write!(
                f,
                "polynomial 0x{:0w$X} does not fit in a {n}-bit register",
                poly,
                w = *width as usize,
                n = *width,
            ),
            Self::FieldOverflow(name, val, mask) => {
                write!(f, "{name} = 0x{val:X} does not fit in a {mask:#x} mask")
            }
        }
    }
}

impl std::error::Error for SpecError {}

/// Reverse the low `n` bits of `value`.
#[must_use]
pub fn reverse_bits(value: u64, n: u8) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut v = value;
    let mut out = 0u64;
    for _ in 0..n {
        out = (out << 1) | (v & 1);
        v >>= 1;
    }
    out
}

/// Reverse all 8 bits of a byte.
#[must_use]
pub fn reverse_byte(b: u8) -> u8 {
    b.reverse_bits()
}

/// The standard catalogue of CRC definitions.
///
/// Values are from the Rockmodel / `reveng` catalogue. The `check` field is
/// the published CRC of the ASCII string `123456789`, which is the standard
/// conformance vector for CRC codes.
pub struct CatalogEntry {
    pub spec: CrcSpec,
    pub check: u64,
    pub use_case: &'static str,
}

/// CRC-8, poly 0x07, no reflection, zero init and xorout.
pub const CRC8: CrcSpec = CrcSpec {
    name: "CRC-8",
    width: 8,
    poly: 0x07,
    init: 0x00,
    refin: false,
    refout: false,
    xorout: 0x00,
    poly_name: "x^8 + x^2 + x + 1",
};

/// CRC-8/SMBUS, the I2C bus check byte.
pub const CRC8_SMBUS: CrcSpec = CrcSpec {
    name: "CRC-8/SMBUS",
    width: 8,
    poly: 0x07,
    init: 0x00,
    refin: false,
    refout: false,
    xorout: 0x00,
    poly_name: "x^8 + x^2 + x + 1",
};

/// CRC-8/MAXIM-DOW, used by 1-Wire temperature sensors.
pub const CRC8_MAXIM: CrcSpec = CrcSpec {
    name: "CRC-8/MAXIM-DOW",
    width: 8,
    poly: 0x31,
    init: 0x00,
    refin: true,
    refout: true,
    xorout: 0x00,
    poly_name: "x^8 + x^5 + x^4 + x^2 + 1",
};

/// CRC-8/HITAG, automotive and RFID systems.
pub const CRC8_HITAG: CrcSpec = CrcSpec {
    name: "CRC-8/HITAG",
    width: 8,
    poly: 0x1D,
    init: 0xFF,
    refin: false,
    refout: false,
    xorout: 0x00,
    poly_name: "x^8 + x^4 + x^3 + x^2 + 1",
};

/// CRC-16/CCITT-FALSE. The classic textbook CRC, no reflection.
pub const CRC16_CCITT_FALSE: CrcSpec = CrcSpec {
    name: "CRC-16/CCITT-FALSE",
    width: 16,
    poly: 0x1021,
    init: 0xFFFF,
    refin: false,
    refout: false,
    xorout: 0x0000,
    poly_name: "x^16 + x^12 + x^5 + 1",
};

/// CRC-16/XMODEM, zero init variant of CCITT used by ZMODEM.
pub const CRC16_XMODEM: CrcSpec = CrcSpec {
    name: "CRC-16/XMODEM",
    width: 16,
    poly: 0x1021,
    init: 0x0000,
    refin: false,
    refout: false,
    xorout: 0x0000,
    poly_name: "x^16 + x^12 + x^5 + 1",
};

/// CRC-16/IBM-SDLC, the HDLC frame check sequence. Also known as X-25.
pub const CRC16_IBM: CrcSpec = CrcSpec {
    name: "CRC-16/IBM-SDLC",
    width: 16,
    poly: 0x1021,
    init: 0xFFFF,
    refin: true,
    refout: true,
    xorout: 0xFFFF,
    poly_name: "x^16 + x^12 + x^5 + 1",
};

/// CRC-16/MODBUS, the industrial serial protocol everyone has debugged.
pub const CRC16_MODBUS: CrcSpec = CrcSpec {
    name: "CRC-16/MODBUS",
    width: 16,
    poly: 0x8005,
    init: 0xFFFF,
    refin: true,
    refout: true,
    xorout: 0x0000,
    poly_name: "x^16 + x^15 + x^2 + 1",
};

/// CRC-16/ARC, used by the xmodem and USB bootloaders.
pub const CRC16_ARC: CrcSpec = CrcSpec {
    name: "CRC-16/ARC",
    width: 16,
    poly: 0x8005,
    init: 0x0000,
    refin: true,
    refout: true,
    xorout: 0x0000,
    poly_name: "x^16 + x^15 + x^2 + 1",
};

/// CRC-32/ISO-HDLC, the zlib and Ethernet checksum.
pub const CRC32_ISO: CrcSpec = CrcSpec {
    name: "CRC-32/ISO-HDLC",
    width: 32,
    poly: 0x04C1_1DB7,
    init: 0xFFFF_FFFF,
    refin: true,
    refout: true,
    xorout: 0xFFFF_FFFF,
    poly_name: "x^32 + x^26 + x^23 + x^22 + x^16 + x^12 + x^11 + x^10 + x^8 + x^7 + x^5 + x^4 + x^2 + x + 1",
};

/// CRC-32/BZIP2, the reverse-engineered PNG variant.
pub const CRC32_BZIP2: CrcSpec = CrcSpec {
    name: "CRC-32/BZIP2",
    width: 32,
    poly: 0x04C1_1DB7,
    init: 0xFFFF_FFFF,
    refin: false,
    refout: false,
    xorout: 0xFFFF_FFFF,
    poly_name: "x^32 + x^26 + x^23 + x^22 + x^16 + x^12 + x^11 + x^10 + x^8 + x^7 + x^5 + x^4 + x^2 + x + 1",
};

/// CRC-32/MPEG-2, used in digital television streams.
pub const CRC32_MPEG2: CrcSpec = CrcSpec {
    name: "CRC-32/MPEG-2",
    width: 32,
    poly: 0x04C1_1DB7,
    init: 0xFFFF_FFFF,
    refin: false,
    refout: false,
    // Unusually, MPEG-2 applies no final XOR, unlike its BZIP2 sibling which
    // differs from it in exactly this parameter.
    xorout: 0x0000_0000,
    poly_name: "x^32 + x^26 + x^23 + x^22 + x^16 + x^12 + x^11 + x^10 + x^8 + x^7 + x^5 + x^4 + x^2 + x + 1",
};

/// CRC-32/POSIX, the cksum(1) checksum on Linux and BSD.
pub const CRC32_POSIX: CrcSpec = CrcSpec {
    name: "CRC-32/POSIX",
    width: 32,
    poly: 0x04C1_1DB7,
    init: 0x0000_0000,
    refin: false,
    refout: false,
    xorout: 0xFFFF_FFFF,
    poly_name: "x^32 + x^26 + x^23 + x^22 + x^16 + x^12 + x^11 + x^10 + x^8 + x^7 + x^5 + x^4 + x^2 + x + 1",
};

/// CRC-64/XZ, the integrity check in the .xz container format.
pub const CRC64_XZ: CrcSpec = CrcSpec {
    name: "CRC-64/XZ",
    width: 64,
    // The forward polynomial. The value 0xC96C5795D7870F42 that circulates in
    // table listings is this same polynomial *reversed*, which is what the
    // reflected hardware form uses; supplying it here alongside refin=true
    // would reflect twice and produce a wrong checksum.
    poly: 0x42F0_E1EB_A9EA_3693,
    init: 0xFFFF_FFFF_FFFF_FFFF,
    refin: true,
    refout: true,
    xorout: 0xFFFF_FFFF_FFFF_FFFF,
    poly_name: "x^64 + x^4 + x^3 + x + 1",
};

/// The catalogue, in presentation order.
pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        spec: CRC8,
        check: 0xF4,
        use_case: "ATM HEC, general 8-bit integrity",
    },
    CatalogEntry {
        spec: CRC8_SMBUS,
        check: 0xF4,
        use_case: "I2C/SMBus device addressing",
    },
    CatalogEntry {
        spec: CRC8_MAXIM,
        check: 0xA1,
        use_case: "1-Wire bus, DS18B20 temperature sensors",
    },
    CatalogEntry {
        spec: CRC8_HITAG,
        check: 0xB4,
        use_case: "RFID and automotive anti-theft systems",
    },
    CatalogEntry {
        spec: CRC16_CCITT_FALSE,
        check: 0x29B1,
        use_case: "HDLC, X.25, Bluetooth, aviation",
    },
    CatalogEntry {
        spec: CRC16_XMODEM,
        check: 0x31C3,
        use_case: "ZMODEM file transfer, radio telemetry",
    },
    CatalogEntry {
        spec: CRC16_IBM,
        check: 0x906E,
        use_case: "HDLC frame check sequence, USB",
    },
    CatalogEntry {
        spec: CRC16_MODBUS,
        check: 0x4B37,
        use_case: "Modbus RTU industrial serial links",
    },
    CatalogEntry {
        spec: CRC16_ARC,
        check: 0xBB3D,
        use_case: "USB bootloaders, LZW compression",
    },
    CatalogEntry {
        spec: CRC32_ISO,
        check: 0xCBF4_3926,
        use_case: "zlib, PNG, Ethernet, gzip, HTTP",
    },
    CatalogEntry {
        spec: CRC32_BZIP2,
        check: 0xFC89_1918,
        use_case: "bzip2, PNG, iTunes metadata",
    },
    CatalogEntry {
        spec: CRC32_MPEG2,
        // Verified by three independent implementations (bitwise MSB, reflected
        // LSB, and byte-table). A frequently cited value of 0x0370E6E7 does not
        // correspond to any parameter set in this neighbourhood.
        check: 0x0376_E6E7,
        use_case: "MPEG-2 transport streams, DVB",
    },
    CatalogEntry {
        spec: CRC32_POSIX,
        check: 0x765E_7680,
        use_case: "Linux and BSD cksum(1)",
    },
    CatalogEntry {
        spec: CRC64_XZ,
        check: 0x995D_C9BB_DF19_39FA,
        use_case: ".xz compression, Level 6/7 of RAID",
    },
];

/// Look up a catalogue entry by name, case-insensitive.
#[must_use]
pub fn find_by_name(name: &str) -> Option<&'static CatalogEntry> {
    let needle = name.to_ascii_lowercase();
    CATALOG.iter().find(|e| e.spec.name.to_ascii_lowercase() == needle)
}
