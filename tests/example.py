#! /usr/bin/env python3

import sys

from blake3 import blake3

# Open the input file, if a command line argument is provided. Otherwise read
# from stdin.
if len(sys.argv) > 1:
    assert len(sys.argv) == 2
    input_file = open(sys.argv[1], "rb")
else:
    input_file = sys.stdin.buffer

# Hash stdin in 64 KiB chunks.
hasher = blake3()
buf = memoryview(bytearray(65536))
while True:
    n = input_file.readinto(buf)
    if n == 0:
        break
    hasher.update(buf[:n])
print(hasher.hexdigest())
