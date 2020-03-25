# blake3-py [![Actions Status](https://github.com/oconnor663/blake3-py/workflows/tests/badge.svg)](https://github.com/oconnor663/blake3-py/actions) [![PyPI version](https://badge.fury.io/py/blake3.svg)](https://pypi.python.org/pypi/blake3)

Python bindings for the [official Rust implementation of
BLAKE3](https://github.com/BLAKE3-team/BLAKE3), based on
[PyO3](https://github.com/PyO3/pyo3). These bindings expose all the
features of BLAKE3, including extendable output, keying, and
multithreading.

**Caution:** This is a brand new library. Please expect some build
issues on platforms not covered by [CI
testing](https://github.com/oconnor663/blake3-py/actions). If you're
using this for anything important, please test your code against
known-good outputs. See also the [soundness
concerns](#thread-safety-and-soundness) below.

# Example

```python
from blake3 import blake3, KEY_LEN, OUT_LEN

# Hash some input all at once.
hash1 = blake3(b"foobarbaz").digest()

# Hash the same input incrementally.
hasher = blake3()
hasher.update(b"foo")
hasher.update(b"bar")
hasher.update(b"baz")
hash2 = hasher.digest()
assert hash1 == hash2

# Hexadecimal output.
print("The hash of 'hello world' is", blake3(b"hello world").hexdigest())

# Use the keyed hashing mode, which takes a 32-byte key.
zero_key = b"\0" * KEY_LEN
message = b"a message to authenticate"
mac = blake3(message, key=zero_key)

# Use the key derivation mode, which takes a context string. Context
# strings should be hardcoded, globally unique, and application-specific.
example_context = "blake3-py 2020-03-04 11:13:10 example context"
key_material = b"some super secret key material"
derived_key = blake3(key_material, context=example_context)

# Extendable output. The default OUT_LEN is 32 bytes.
extended = blake3(b"foo").digest(length=100)
assert extended[:OUT_LEN] == blake3(b"foo").digest()
assert extended[75:100] == blake3(b"foo").digest(length=25, seek=75)

# Hash a large input with multithreading. Note that this can be slower
# for short inputs, and you should benchmark it for your use case on
# your platform. As a rule of thumb, don't use multithreading for inputs
# shorter than 1 MB.
large_input = bytearray(1_000_000)
hash3 = blake3(large_input, multithreading=True)
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
you'll need nightly Rust installed. (If you use
[rustup](https://rustup.rs/), it will read the `rust-toolchain` file in
this project and use the nightly toolchain automatically.)

# Thread Safety and Soundness

This wrapper is not currently thread-safe. Like the hash implementations
in the Python standard library, we release the GIL during `update`, to
avoid blocking the entire process. However, that means that calling the
`update` method on the same object from multiple threads at the same
time is undefined behavior. We could solve this by putting a `Mutex`
inside the wrapper type, but I'd like to get some expert advice about
the best practice here first.

A deeper problem is that another thread might mutate a `bytearray` while
we're hashing it, and while our Rust code is treating it as a `&[u8]`.
That violates Rust's aliasing guarantees and is also technically
undefined behavior. However, the only possible way to solve this while
still supporting `bytearray` would be to retain the GIL. Again, I'm in
need of expert advice.

These concerns are more theoretical than practical, however. If you're
racing to update a hasher, or racing to hash a buffer while it's being
written to, the result is inherently nondeterministic. That's almost
certainly a bug in your program, whether or not it's technically sound.
