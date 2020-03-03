# blake3-py [![Actions Status](https://github.com/oconnor663/blake3-py/workflows/tests/badge.svg)](https://github.com/oconnor663/blake3-py/actions) [![PyPI version](https://badge.fury.io/py/blake3.svg)](https://pypi.python.org/pypi/blake3)

Python bindings for the Rust [`blake3`](https://crates.io/crates/blake3)
crate, based on [PyO3](https://github.com/PyO3/pyo3). This a proof of
concept, not yet fully-featured or production-ready. See also the
[soundness concerns](#thread-safety-and-soundness) below.

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

As usual with Pip, you might need to use `sudo` or the `--user` flag
with the command above, depending on how you installed Python on your
system.

There are binary wheels [available on
PyPI](https://pypi.org/project/blake3/#files) for most environments, so
most users do not need a Rust toolchain. If you're building the source
distribution, or if a binary wheel isn't available for your environment,
you'll need the **nightly** Rust toolchain (required by PyO3). This
project includes a `rust-toolchain` file, so
[rustup](https://rustup.rs/) will install and invoke the nightly
toolchain automatically.

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

# Thread Safety and Soundness

This wrapper is not currently thread-safe. Like the hash implementations
in the Python standard library, we release the GIL during `update`, to
avoid blocking the entire process. However, that means that calling the
`update` method from multiple threads at the same time is undefined
behavior. We could solve this by putting a `Mutex` inside the wrapper
type, but I'd like to get some expert advice about the best practice
here first.

A deeper problem is that another thread might mutate a `bytearray` while
we're hashing it, and while our Rust code is treating it as a `&[u8]`.
That violates Rust's aliasing guarantees and is also technically
undefined behavior. However, the only possible way to solve this while
still supporting `bytearray` would be to retain the GIL. Again, I'm in
need of expert advice.
