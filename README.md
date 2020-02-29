# blake3-py [![Actions Status](https://github.com/oconnor663/blake3-py/workflows/tests/badge.svg)](https://github.com/oconnor663/blake3-py/actions) [![PyPI version](https://badge.fury.io/py/blake3.svg)](https://pypi.python.org/pypi/blake3)

Python bindings for the Rust [`blake3`](https://crates.io/crates/blake3)
crate, based on [PyO3](https://github.com/PyO3/pyo3). This a proof of
concept, not yet fully-featured or production-ready. See also the
[Soundness](#soundness) concerns below.

# Example

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

If you've cloned the [GitHub
project](https://github.com/oconnor663/blake3-py), and you want to
experiment with the scripts there, they work like this:

```bash
# Build the shared library first.
$ ./build.py

# Hash some input.
$ echo hello world | ./example.py
dc5a4edb8240b018124052c330270696f96771a63b45250a5c17d3000e823355

# Run the tests.
$ ./test.py
```

# Installation

```
pip install blake3
```

There may be a binary wheel
[available](https://pypi.org/project/blake3/#files) for your system, in
which case installation will be quick and you don't need to have Rust
installed. If a wheel isn't available for your system, you'll need the
**nightly** Rust toolchain installed to compile things locally. (PyO3
currently requires nightly. The
[maturin](https://github.com/PyO3/maturin) build tool, which is invoked
automatically by `pip install`, might install nightly Rust for you.)

As usual with Pip, you might need to use `sudo` or the `--user` flag
with the command above, depending on how Python is installed.

# Soundness

There are some fundamental questions about whether these bindings can be
sound. Like the Python standard library's hash implementations, in order
to avoid blocking other threads during a potentially expensive call to
`update()`, we release the GIL. But that opens up the possibility that
another thread might mutate, say, the `bytearray` we're hashing, while
the Rust code is treating it as a `&[u8]`. That violates Rust's aliasing
guarantees and is technically unsound. However, no Python hashing
implementation that I'm aware of holds the GIL while it calls into
native code. I need to get some expert opinions on this.

# Features

Currently only basic hashing is supported, with the default 32-byte
output size. Missing BLAKE3 features should be easy to add, though I'm
not sure exactly what the API should look like. Missing features
include:

- variable-length output
- an incremental output reader
- the keyed hashing mode
- the key derivation mode
- optional multi-threading
