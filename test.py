#! /usr/bin/env python3

from os import path
import subprocess

HERE = path.dirname(__file__)

subprocess.run(["cargo", "build", "--release"], check=True, cwd=HERE)

# This works because ./blake3.so is a symlink to ./target/release/libblake3.so.
import blake3  # noqa: E261

hello_hash = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"

print("test all at once")
assert blake3.blake3(b"hello world").hexdigest() == hello_hash
assert blake3.blake3(bytearray(b"hello world")).hexdigest() == hello_hash
assert blake3.blake3(memoryview(b"hello world")).hexdigest() == hello_hash

print("test incremental")
hasher = blake3.blake3()
hasher.update(b"hello")
hasher.update(bytearray(b" "))
hasher.update(memoryview(b"world"))
assert hasher.hexdigest() == hello_hash
assert hasher.digest().hex() == hello_hash
