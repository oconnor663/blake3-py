#! /usr/bin/env python3

import sys
from os import path
import subprocess

HERE = path.dirname(__file__)

subprocess.run(["cargo", "build", "--release", "--quiet"],
               check=True,
               cwd=HERE)

# This works because ./blake3.so is a symlink to ./target/release/libblake3.so.
import blake3  # noqa: E261

# Hash stdin in 64 KiB chunks.
hasher = blake3.blake3()
buf = memoryview(bytearray(65536))
while True:
    n = sys.stdin.buffer.readinto(buf)
    if n == 0:
        break
    hasher.update(buf[:n])
print(hasher.hexdigest())
