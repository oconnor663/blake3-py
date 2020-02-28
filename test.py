#! /usr/bin/env python3

# Run ./build.py first, which puts the blake3 shared library in this directory.
import blake3
from os import path
import subprocess
import sys

HERE = path.dirname(__file__) or "."

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

print("test incorrect argument type")
try:
    blake3.blake3("a string")
except TypeError:
    pass
else:
    assert False, "expected a type error"

print("test example.py")
output = subprocess.run(
    [sys.executable, path.join(HERE, "example.py")],
    check=True,
    input=b"hello world",
    stdout=subprocess.PIPE).stdout.decode().strip()
assert output == hello_hash
