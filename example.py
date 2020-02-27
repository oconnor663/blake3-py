#! /usr/bin/env python3

from os import path
import subprocess

HERE = path.dirname(__file__)

subprocess.run(["cargo", "build", "--release"], check=True, cwd=HERE)

import blake3  # noqa: E261

hasher = blake3.blake3()
hasher.update(b"hello world")
assert hasher.digest() == blake3.blake3(b"hello world").digest()
print(hasher.digest())
print(hasher.hexdigest())
