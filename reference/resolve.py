"""Resolve the three catalogue outliers against a third, table-driven method.

Table-driven byte-at-a-time is how real hardware computes CRCs, and it shares
no code with either bitwise formulation above.
"""

def rev(v, n):
    r = 0
    for _ in range(n):
        r = (r << 1) | (v & 1)
        v >>= 1
    return r


def build_table(width, poly):
    top = 1 << (width - 1)
    tbl = []
    for i in range(256):
        c = i << (width - 8)
        for _ in range(8):
            c = ((c << 1) ^ poly) if (c & top) else (c << 1)
        tbl.append(c & ((1 << width) - 1))
    return tbl


def crc_table(width, poly, init, refin, refout, xorout, data):
    tbl = build_table(width, poly)
    reg = init
    for byte in data:
        if refin:
            byte = rev(byte, 8)
        idx = ((reg >> (width - 8)) ^ byte) & 0xFF
        reg = (reg << 8) ^ tbl[idx]
        reg &= (1 << width) - 1
    if refout:
        reg = rev(reg, width)
    return (reg ^ xorout) & ((1 << width) - 1)


msg = b"123456789"

print("=== Hypothesis 1: CRC-16/IBM-SDLC needs xorout=0xFFFF (X-25 alias) ===")
for xo in (0x0000, 0xFFFF):
    v = crc_table(16, 0x1021, 0xFFFF, True, True, xo, msg)
    print(f"  xorout=0x{xo:04X} -> 0x{v:04X}   {'<-- 0x906E MATCH' if v == 0x906E else ''}")

print("\n=== Hypothesis 2: CRC-32/MPEG-2 params, sweeping xorout ===")
for xo in (0x00000000, 0xFFFFFFFF):
    v = crc_table(32, 0x04C11DB7, 0xFFFFFFFF, False, False, xo, msg)
    print(f"  xorout=0x{xo:08X} -> 0x{v:08X}   {'<-- 0x0370E6E7 MATCH' if v == 0x0370E6E7 else ''}")

print("\n=== Hypothesis 3: CRC-64/XZ uses the non-reflected poly 0x42F0E1EBA9EA3693 ===")
for poly in (0xC96C5795D7870F42, 0x42F0E1EBA9EA3693):
    for refin, refout in ((True, True), (False, False)):
        v = crc_table(64, poly, 0xFFFFFFFFFFFFFFFF, refin, refout, 0xFFFFFFFFFFFFFFFF, msg)
        tag = "  <-- 0x995DC9BBDF1939FA MATCH" if v == 0x995DC9BBDF1939FA else ""
        print(f"  poly=0x{poly:016X} refin={refin!s:5} -> 0x{v:016X}{tag}")

print("\n=== Confirm: table vs bitwise agree on the fixed entries ===")
checks = [
    ("CRC-16/IBM-SDLC", 16, 0x1021, 0xFFFF, True, True, 0xFFFF, 0x906E),
    ("CRC-32/MPEG-2", 32, 0x04C11DB7, 0xFFFFFFFF, False, False, 0x00000000, 0x0370E6E7),
    ("CRC-64/XZ", 64, 0x42F0E1EBA9EA3693, 0xFFFFFFFFFFFFFFFF, True, True, 0xFFFFFFFFFFFFFFFF, 0x995DC9BBDF1939FA),
]
for name, w, p, init, ri, ro, xo, expect in checks:
    v = crc_table(w, p, init, ri, ro, xo, msg)
    print(f"  {name:20} 0x{v:X} expected 0x{expect:X}  {'OK' if v == expect else 'STILL WRONG'}")
