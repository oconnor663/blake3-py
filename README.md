# blake3-py [![tests](https://github.com/oconnor663/blake3-py/actions/workflows/tests.yml/badge.svg?branch=master&event=push)](https://github.com/oconnor663/blake3-py/actions/workflows/tests.yml) [![PyPI version](https://badge.fury.io/py/blake3.svg)](https://pypi.python.org/pypi/blake3)

Python bindings for the [official Rust implementation of
BLAKE3](https://github.com/BLAKE3-team/BLAKE3), based on
[PyO3](https://github.com/PyO3/pyo3). These bindings expose all the features of
BLAKE3, including extendable output, keying, and multithreading. The basic API
matches that of Python's standard
[`hashlib`](https://docs.python.org/3/library/hashlib.html) module.

## Examples

```python
from blake3 import blake3

# Hash some input all at once. The input can be bytes, a bytearray, or a memoryview.
hash1 = blake3(b"foobarbaz").digest()

# Hash the same input incrementally.
hasher = blake3()
hasher.update(b"foo")
hasher.update(b"bar")
hasher.update(b"baz")
hash2 = hasher.digest()
assert hash1 == hash2

# Hash the same input fluently.
assert hash1 == blake3(b"foo").update(b"bar").update(b"baz").digest()

# Hexadecimal output.
print("The hash of 'hello world' is", blake3(b"hello world").hexdigest())

# Use the keyed hashing mode, which takes a 32-byte key.
import secrets
random_key = secrets.token_bytes(32)
message = b"a message to authenticate"
mac = blake3(message, key=random_key).digest()

# Use the key derivation mode, which takes a context string. Context strings
# should be hardcoded, globally unique, and application-specific.
context = "blake3-py 2020-03-04 11:13:10 example context"
key_material = b"usually at least 32 random bytes, not a password"
derived_key = blake3(key_material, derive_key_context=context).digest()

# Extendable output. The default digest size is 32 bytes.
extended = blake3(b"foo").digest(length=100)
assert extended[:32] == blake3(b"foo").digest()
assert extended[75:100] == blake3(b"foo").digest(length=25, seek=75)

# Hash a large input using multiple threads. Note that this can be slower for
# inputs shorter than ~1 MB, and it's a good idea to benchmark it for your use
# case on your platform.
large_input = bytearray(1_000_000)
hash_single = blake3(large_input).digest()
hash_two = blake3(large_input, max_threads=2).digest()
hash_many = blake3(large_input, max_threads=blake3.AUTO).digest()
assert hash_single == hash_two == hash_many

# Hash a file with multiple threads using memory mapping. This is what b3sum
# does by default.
file_hasher = blake3(max_threads=blake3.AUTO)
file_hasher.update_mmap("/big/file.txt")
file_hash = file_hasher.digest()

# Copy a hasher that's already accepted some input.
hasher1 = blake3(b"foo")
hasher2 = hasher1.copy()
hasher1.update(b"bar")
hasher2.update(b"baz")
assert hasher1.digest() == blake3(b"foobar").digest()
assert hasher2.digest() == blake3(b"foobaz").digest()
```

## Installation

```
pip install blake3
```

As usual with Pip, you might need to use `sudo` or the `--user` flag
with the command above, depending on how you installed Python on your
system.

There are binary wheels [available on
PyPI](https://pypi.org/project/blake3/#files) for most environments. But
if you're building the source distribution, or if a binary wheel isn't
available for your environment, you'll need to [install the Rust
toolchain](https://rustup.rs).

## C Bindings

Experimental bindings for the official BLAKE3 C implementation are available in
the [`c_impl`](c_impl) directory. These will probably not be published on PyPI,
and most applications should prefer the Rust-based bindings. But if you can't
depend on the Rust toolchain, and you're on some platform that this project
doesn't provide binary wheels for, the C-based bindings might be an
alternative.
