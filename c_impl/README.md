# Python bindings for the BLAKE3 C implementation

This directory contains a reimplementation of the Python
[`blake3`](https://pypi.org/project/blake3) module, based on the official C
implementation of BLAKE3 rather than the official Rust implementation. This is
mainly intended as a proof-of-concept for code that might be submitted to
Python's hashlib in the future, and this module probably won't be published to
PyPI. Most applications should prefer the Rust-based bindings.

The C implementation of BLAKE3 doesn't currently support multithreading, but
the `max_threads` parameter is still allowed (and ignored) in these bindings
for compatibility with the Rust version. This point of compatibility is
actually why the parameter was named `max_threads` in the first place. Credit
to Larry Hastings for that idea.

## Building

The build is defined in [`setup.py`](setup.py), and you can compile and install
this module with the usual commands like

```
pip install .
```

Python 3.8 or later is required.

The build implements the following decision tree, which is [documented in
greater detail
upstream](https://github.com/BLAKE3-team/BLAKE3/tree/master/c#building):

- Are we building on macOS?
    - Combine the Unix assembly files and the NEON intrinsics implementation
      into a "universal" binary using the `lipo` tool.
- Are we targetting x86-64?
    - Are we targetting Windows?
        - Include the MSVC assembly files.
    - Otherwise:
        - Include the Unix assembly files.
- Are we targetting 32-bit x86?
    - Build and include the x86 intrinsics files. These are OS-independent, but
      each one needs to be compiled with different flags (`-msse4.1`, `-mavx2`,
      etc.) to enable the appropriate instruction set extensions.
- Are we targetting AArch64?
    - Build and include the NEON intrinsics implementation. There are ARMv7
      targets that support NEON, but there are also ARMv7 targets that don't,
      and there's no standard way to distinguish them. So we play it safe and
      only enable NEON on AArch64.
- Otherwise:
    - Just build the portable implementation.

The x86-64-only assembly implementations perform better than intrinsics, and
they also build faster, so these are preferable where possible. They come in
three target-specific flavors: Unix, Windows MSVC, and Windows GNU. Currently
`setup.py` doesn't try to target the Windows GNU ABI, but this should be
relatively easy to add if needed.

Nothing here takes cross-compilation into account yet. Can Python extensions be
cross-compiled? Do we care about this?

**Feedback needed:** I'm very new to both `setuptools` and the Python C API,
and there's a good chance I've made mistakes or accidentally relied on
deprecated features. This code needs to be reviewed by someone with more
experience. The part that sets compiler flags for intrinsics files and the part
that builds `.asm` files on Windows are especially rocky.
