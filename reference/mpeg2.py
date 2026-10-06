"""Brute-force the MPEG-2 check value to find which parameter set, if any,
produces 0x0370E6E7. If none does, the recalled constant is the error."""

def rev(v, n):
    r = 0
    for _ in range(n):
        r = (r << 1) | (v & 1)
        v >>= 1
    return r


def crc(width, poly, init, refin, refout, xorout, data):
    top = 1 << (width - 1)
    mask = (1 << width) - 1
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


msg = b"123456789"
TARGET = 0x0370E6E7
CAND = 0x0376E6E7

print("Searching the MPEG-2 parameter neighbourhood for 0x0370E6E7:\n")
found = []
polys = [0x04C11DB7, 0x1EDC6F41, 0xA833982B]
inits = [0x00000000, 0xFFFFFFFF]
xouts = [0x00000000, 0xFFFFFFFF]
for p in polys:
    for init in inits:
        for ri, ro in ((False, False), (True, True)):
            for xo in xouts:
                v = crc(32, p, init, ri, ro, xo, msg)
                if v == TARGET:
                    found.append((p, init, ri, ro, xo))
                print(f"  poly=0x{p:08X} init=0x{init:08X} refin={ri!s:5} xorout=0x{xo:08X} -> 0x{v:08X}")

print(f"\nCandidates producing the target: {found if found else 'NONE'}")

# If nothing reaches the target, the target constant itself is wrong. Confirm
# by checking which entries share our value, as a sanity anchor.
print(f"\nOur value 0x{CAND:08X}. Note CRC-32/MPEG-2 shares BZIP2's poly and init,")
print("differing only in xorout. BZIP2 check = 0xFC891918, MPEG-2 = our computed value.")
bzip2 = crc(32, 0x04C11DB7, 0xFFFFFFFF, False, False, 0xFFFFFFFF, msg)
mpeg2 = crc(32, 0x04C11DB7, 0xFFFFFFFF, False, False, 0x00000000, msg)
print(f"  BZIP2  0x{bzip2:08X}  (published 0xFC891918)  {'OK' if bzip2 == 0xFC891918 else 'WRONG'}")
print(f"  MPEG-2 0x{mpeg2:08X}")
print(f"\n  XOR relationship: 0x{0x0376E6E7 ^ 0xFC891918:08X} "
      f"({'equals init/xorout delta 0xFFFFFFFF' if (mpeg2 ^ bzip2) == 0xFFFFFFFF else 'unexplained'})")
