# blake3-py [![Actions Status](https://github.com/oconnor663/blake3-py/workflows/tests/badge.svg)](https://github.com/oconnor663/blake3-py/actions)

A python wrapper around the Rust
[`blake3`](https://crates.io/crates/blake3) crate, based on
[PyO3](https://github.com/PyO3/pyo3). This is a minimal proof of
concept, currently Linux-only. I'm going to have to get more familiar
with Python packaging to make this production-ready. See also the
[Soundness](#soundness) concerns below.

# Example

How to try out this repo on the command line:

```bash
# You have to build the shared library first.
$ ./build.py

# Try out example.py.
$ echo hello world | ./example.py
dc5a4edb8240b018124052c330270696f96771a63b45250a5c17d3000e823355

# Run a few tests.
$ ./test.py
```

What it looks like to use `blake3` in Python code:

```python
import blake3

hash1 = blake3.blake3(b"foobarbaz").digest()

hasher = blake3.blake3()
hasher.update(b"foo")
hasher.update(b"bar")
hasher.update(b"baz")
hash2 = hasher.digest()

assert hash1 == hash2

print("The hash of 'hello world' is:",
      blake3.blake3(b"hello world").hexdigest())
```

# Building

The `build.py` script runs `cargo build --release` and then copies the
resulting shared library to a platform-appropriate name (`blake3.so` on
Linux/macOS, and `blake3.pyd` on Windows) in the repo root directory.
Python scripts in that directory will then load the shared library when
they `import blake3`.

This project is not yet packaged in a way that's convenient to `pip
install`. I need to learn more about Python packaging to understand the
right way to do this. (Binary wheels?) Any help on this front from folks
with more experience would be greatly appreciated.

# Soundness

There are some fundamental questions about whether this wrapper can be
sound. Like the Python standard library's hash implementations, in order
to avoid blocking other threads during a potentially expensive call to
`update()`, this wrapper releases the GIL. But that opens up the
possibility that another thread might mutate, say, the `bytearray` we're
hashing, while the Rust code is treating it as a `&[u8]`. That violates
Rust's aliasing guarantees and is technically unsound. However, no
Python hashing implementation that I'm aware of holds the GIL while it
calls into native code. I'm in need of some expert opinions on this.

# Features

Currently only basic hashing is supported, with the default 32-byte
output size. Missing BLAKE3 features should be easy to add, though I'm
not sure exactly what the API should look like. Missing features
include:

- variable-length output
- an incremental output reader
- the keyed hashing mode
- the key derivation mode
