"""Independent CRC reference, written from the Rockmodel definition.

Deliberately uses the reflected LSB-first formulation, which shares no code
with the MSB-first implementation under test. Agreement between the two is
evidence; agreement with a shared bug is not.
"""

def rev(v, n):
    r = 0
    for _ in range(n):
        r = (r << 1) | (v & 1)
        v >>= 1
    return r


def crc_msb(width, poly, init, refin, refout, xorout, data):
    mask = (1 << width) - 1
    top = 1 << (width - 1)
    reg = init & mask
    for byte in data:
        b = rev(byte, 8) if refin else byte
        reg ^= b << (width - 8)
        for _ in range(8):
            reg = ((reg << 1) ^ poly) if (reg & top) else (reg << 1)
            reg &= mask
    if refout:
        reg = rev(reg, width)
    return (reg ^ xorout) & mask


def crc_lsb(width, poly, init, refin, refout, xorout, data):
    """Reflected hardware formulation, as used for table-driven CRC-32.

    The input byte is XORed in directly with no reflection of its own; the
    reflection is already baked into the reversed polynomial. Valid only for
    refin=true, which is exactly where it is useful as a cross-check.
    """
    poly_rev = rev(poly, width)
    reg = init
    for byte in data:
        reg ^= byte
        for _ in range(8):
            reg = (reg >> 1) ^ poly_rev if (reg & 1) else (reg >> 1)
    return reg ^ xorout


def poly_remainder(bits, gen_bits):
    """Textbook long division of an augmented bit list by the generator."""
    work = list(bits)
    m = len(work) - len(gen_bits) + 1
    for i in range(m):
        if work[i]:
            for j, g in enumerate(gen_bits):
                work[i + j] ^= g
    return work[m - 1:]


CATALOG = [
    ("CRC-8",            8,  0x07,        0x00,       False, False, 0x00,        0xF4),
    ("CRC-8/SMBUS",      8,  0x07,        0x00,       False, False, 0x00,        0xF4),
    ("CRC-8/MAXIM-DOW",  8,  0x31,        0x00,       True,  True,  0x00,        0xA1),
    ("CRC-8/HITAG",      8,  0x1D,        0xFF,       False, False, 0x00,        0xB4),
    ("CRC-16/CCITT-FALSE",16, 0x1021,      0xFFFF,     False, False, 0x0000,      0x29B1),
    ("CRC-16/XMODEM",    16, 0x1021,      0x0000,     False, False, 0x0000,      0x31C3),
    ("CRC-16/IBM-SDLC",  16, 0x1021,      0xFFFF,     True,  True,  0x0000,      0x906E),
    ("CRC-16/MODBUS",    16, 0x8005,      0xFFFF,     True,  True,  0x0000,      0x4B37),
    ("CRC-16/ARC",       16, 0x8005,      0x0000,     True,  True,  0x0000,      0xBB3D),
    ("CRC-32/ISO-HDLC",  32, 0x04C11DB7,  0xFFFFFFFF, True,  True,  0xFFFFFFFF,  0xCBF43926),
    ("CRC-32/BZIP2",     32, 0x04C11DB7,  0xFFFFFFFF, False, False, 0xFFFFFFFF,  0xFC891918),
    ("CRC-32/MPEG-2",    32, 0x04C11DB7,  0xFFFFFFFF, False, False, 0x00000000, 0x0370E6E7),
    ("CRC-32/POSIX",     32, 0x04C11DB7,  0x00000000, False, False, 0xFFFFFFFF, 0x765E7680),
    ("CRC-64/XZ",        64, 0xC96C5795D7870F42, 0xFFFFFFFFFFFFFFFF, True, True, 0xFFFFFFFFFFFFFFFF, 0x995DC9BBDF1939FA),
]

msg = b"123456789"
print(f"{'name':22} {'msb':>18} {'lsb':>18}  match  catalogue")
bad = 0
for name, w, p, init, ri, ro, xo, chk in CATALOG:
    a = crc_msb(w, p, init, ri, ro, xo, msg)
    # The LSB form is only equivalent when the input is reflected.
    b = crc_lsb(w, p, init, ri, ro, xo, msg) if ri else None
    agree = b is None or a == b
    match = a == chk
    if not (agree and match):
        bad += 1
    bs = f"{b:>18X}" if b is not None else f"{'n/a':>18}"
    print(f"{name:22} {a:>18X} {bs}  {agree!s:5}  {chk:>018X} {'OK' if match else 'MISMATCH'}")

print(f"\n{bad} problem(s)")

# Cross-check the pure long-division remainder against the register for the
# init=0, xorout=0 case, where the two definitions must coincide exactly.
print("\nlong division vs register (init=0, xorout=0):")
for name, w, p, init, ri, ro, xo, chk in CATALOG:
    if init != 0 or xo != 0:
        continue
    gen = [(p >> (w - i)) & 1 for i in range(w + 1)]
    for payload in (b"A", b"Networking", b"hello world", b"\x00\xff\x0f"):
        bits = []
        for byte in payload:
            bits += [(byte >> i) & 1 for i in range(7, -1, -1)]
        aug = bits + [0] * w
        rem = poly_remainder(aug, gen)
        rd = int("".join(str(b) for b in rem), 2) if rem else 0
        reg = crc_msb(w, p, 0, ri, ro, 0, payload)
        flag = "OK" if rd == reg else f"MISMATCH rd={rd:X} reg={reg:X}"
        print(f"  {name:22} {payload!s:16} {flag}")
        break
