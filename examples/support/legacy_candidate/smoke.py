#!/usr/bin/env python3
"""Seconds-long worker protocol/binding check; not a v5 compatibility claim."""
import subprocess
import sys

MASK = (1 << 64) - 1

def rol(x, n):
    return ((x << n) | (x >> (64 - n))) & MASK

def ror(x, n):
    return ((x >> n) | (x << (64 - n))) & MASK

def bind(basis, file_id):
    z2 = int.from_bytes(basis[:8], "little")
    z1 = int.from_bytes(basis[8:], "little")
    z4 = int.from_bytes(file_id[:8], "little")
    z3 = int.from_bytes(file_id[8:16], "little")
    for i in range(32):
        z1 = z4 ^ ((ror(z1, 8) + z2) & MASK)
        z2 = z1 ^ rol(z2, 3)
        z3 = i ^ ((ror(z3, 8) + z4) & MASK)
        z4 = z3 ^ rol(z4, 3)
    return z2.to_bytes(8, "little") + z1.to_bytes(8, "little")

for basis, file_id in [(bytes(16), bytes(20)), (bytes(range(16)), bytes(range(20)))]:
    packet = b"LPPCAN01" + bytes(3072) + basis + file_id
    result = subprocess.run([sys.argv[1]], input=packet, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, timeout=10, env={})
    assert result.returncode == 0 and len(result.stdout) == 32, "worker protocol failed"
    assert result.stdout[:16] == bind(result.stdout[16:], file_id), "binding differs"
for packet in [b"", b"bad", b"LPPCAN01" + bytes(3109)]:
    result = subprocess.run([sys.argv[1]], input=packet, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, timeout=10, env={})
    assert result.returncode != 0 and not result.stdout, "invalid input accepted"
print("PASS: bounded worker protocol and separate binding reference; not v5 compatibility")
