# blake3-py

A python wrapper around the Rust
[`blake3`](https://crates.io/crates/blake3) crate, based on
[PyO3](https://github.com/PyO3/pyo3). This is a minimal proof of
concept, currently Linux-only. I'm going to have to get more familiar
with Python packaging to make this production-ready.

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

# Building

This works best on Linux. First, run `cargo build --release`. Then
rename the file `./target/release/libblake3.so` to `blake3.so` and put
it somewhere where Python can import it. The easiest thing is to have it
in the same directory as your script. Note that `./blake3.so` in this
repo is a symlink to `./target/release/libblake3.so`, to make the
`example.py` and `test.py` scripts work.

In theory this should be able to support macOS and Windows without too
much work. The [PyO3
docs](https://github.com/PyO3/pyo3#using-rust-from-python) mention that
macOS will require some extra linker flags, and I haven't tested it. I
have gotten Windows to work, though the automatic build code in
`example.py` and `test.py` doesn't currently do the right thing, and you
have to manually copy the `.dll` to `blake3.pyd`. Note that you're
likely to run into [issue #712](https://github.com/PyO3/pyo3/issues/712)
on Window, if you have 64-bit Rust and 32-bit Python installed (both of
which are the default).

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
