This directory contains a reimplementation of the Python BLAKE3 bindings, based
on C rather than Rust. This is mainly intended as a proof-of-concept for code
that might be submitted to hashlib in the future, and it probably won't be
published to PyPI.

The official C implementation of BLAKE3 doesn't currently support
multithreading, but the `max_threads` parameter is still available (and
ignored) in these bindings for compatibility with the Rust version. This
compatibility story is in fact the original motivation for naming the parameter
`max_threads`. Credit for this design goes to Larry Hastings.
