extern crate blake3 as upstream_blake3;

use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{PyBufferError, PyOverflowError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyString};
use std::path::PathBuf;
use std::sync::Mutex;

// This is the same as HASHLIB_GIL_MINSIZE in CPython.
const GIL_MINSIZE: usize = 2048;

// We want to support buffers of both signed and unsigned bytes, and for hashing
// purposes we'll pointer cast both to &[u8]. PyO3 gives us typed buffers, so we
// use this enum to wrap them.
enum BytesPyBuffer {
    U8(PyBuffer<u8>),
    I8(PyBuffer<i8>),
}

impl BytesPyBuffer {
    /// Try to get a BytesPyBuffer from any Python object. This succeeds for
    /// Python types like `bytes` and `bytearray` that support the buffer
    /// protocol, but it fails for most other types.
    fn get(data: &Bound<PyAny>) -> PyResult<Self> {
        // First see if we can get a u8 buffer. This is the common case.
        match PyBuffer::<u8>::get(data) {
            Ok(pybuffer) => Ok(Self::U8(pybuffer)),
            // If not, then see if we can get an i8 buffer.
            Err(u8_err) => match PyBuffer::<i8>::get(data) {
                Ok(pybuffer) => Ok(Self::I8(pybuffer)),
                // That didn't work either. Return the first error from above.
                // If they're different, the first one is more likely to be
                // relevant to the caller.
                Err(_) => Err(u8_err),
            },
        }
    }

    /// Get a &[u8] from a PyBuffer<u8> or a PyBuffer<i8>. This function is
    /// unsafe, because the returned slice could be mutably aliased. See the
    /// comments below.
    unsafe fn as_bytes(&self) -> PyResult<&[u8]> {
        match self {
            // These arms look identical, but note that they call
            // bytes_from_pybuffer with different type parameters. That's why
            // the enum is needed.
            Self::U8(pybuffer) => unsafe { bytes_from_pybuffer(pybuffer) },
            Self::I8(pybuffer) => unsafe { bytes_from_pybuffer(pybuffer) },
        }
    }
}

unsafe fn bytes_from_pybuffer<T: pyo3::buffer::Element>(pybuffer: &PyBuffer<T>) -> PyResult<&[u8]> {
    // This function lets us support both signed and unsigned byte arrays
    // without copy-pasting too much code. Assert that we don't use it for
    // anything larger than a byte. There's nothing necessarily evil about
    // hashing an array of u32's, but it would raise endianness issues, and in
    // any case we don't plan to do it.
    assert_eq!(std::mem::size_of::<T>(), 1);

    // Check that the buffer is contiguous. Regular bytes/bytearray types are
    // always contiguous, but e.g. some NumPy arrays can be "strided", and we
    // want to reject those. This is the same check that `PyBuffer::as_slice`
    // does internally.
    if !pybuffer.is_c_contiguous() {
        return Err(PyBufferError::new_err("buffer is not contiguous"));
    }

    // Construct a &[u8] slice over the buffer. This is different from
    // `PyBuffer::as_slice` in a couple ways:
    //
    // - We're potentially pointer-casting from &[i8] to &[u8]. This is fine,
    //   and there are safe APIs for this sort of thing (e.g. `bytemuck`).
    // - `as_slice` returns `&[ReadOnlyCell<T>]` instead of `&[T]`, because the
    //   slice might be mutably aliased. We're exposing &[u8] directly. This is
    //   not obviously fine, and we need to be careful.
    //
    // There are several different types of issues that `PyBuffer` (which we're
    // keeping) and `ReadOnlyCell` (which we're casting away) protect us from:
    //
    // - Someone might try to *resize* the buffer while we're reading it, making
    //   our reads UB. The `PyBuffer` "locks" the buffer to prevent this as long
    //   as we keep it alive. This slice is lifetime-bound to the `PyBuffer`
    //   once we return it, so we're ok here.
    // - We might accidentally write to this buffer ourselves, by writing to a
    //   "different" buffer that turns out to be an alias, or by running any
    //   Python code we don't control that happens to do the same thing. We
    //   won't do any writes to buffers we didn't create internally, and we
    //   won't run any Python code (even finalizers) while this slice is alive,
    //   so I think we're ok here too. (We also won't use any invariant-checking
    //   functions like `std::str::from_utf8` that could get fooled by illegal
    //   mutable aliasing.)
    // - Another thread might do the same thing, totally out of our control.
    //   In addition to problems that we can cause by doing this ourselves, this
    //   is also a *data race*, which is per se UB.
    //
    // The third problem above is the complicated one, because we need to think
    // about the GIL:
    //
    // - The GIL normally blocks other Python threads from running while our
    //   Rust code is running. That would make this whole problem go away.
    // - However, we don't *want* to block other Python threads while we hash
    //   large inputs, so like the C code in Python's `hashlib` module we
    //   release the GIL, which makes the problem come back.
    // - Even if we didn't release the GIL, other Rust extensions or C
    //   extensions might release it, and then we might race with them.
    // - Freethreaded builds of Python 3.13+ don't even have a GIL, so the
    //   question of whether anyone releases it is kind of irrelevant.
    //
    // I think the right way to think about this is to forget about the GIL and
    // just ask what happens if another thread writes to the bytes that we're
    // hashing, while we're hashing them. Clearly part of the story is that
    // we'll read and hash "junk bytes" (some unpredictable mix of before and
    // after, maybe even an "out of thin air" read under exotic circumstances).
    // That much is expected, and it's almost certainly a bug that the caller
    // need to fix on their end. But the more vexing question is, can anything
    // *else* happen? Could this data race trigger "real" UB like corrupting
    // arbitrary memory or executing an attacker's bitcoin mining virus?
    //
    // This is a thorny problem, and the relevant standards make no guarantees.
    // However, it isn't just our problem. The C implementations in Python's
    // `hashlib` have the exact same behavior, and you can trigger a real data
    // race with standard types like this:
    // https://gist.github.com/oconnor663/c69cb4dbffb9b13bbced3fe8ce2181ac.
    // This data race violates the requirements of the C memory model also.
    //
    // At the end of the day, even if this race turns out to be exploitable
    // (which I think is unlikely), only pathological programs should be able to
    // trigger it. Writing to a buffer concurrently from another thread while
    // hashing it is a very weird thing to do, and it's almost guaranteed to be
    // a correctness bug, regardless of whether it's also a soundness bug.
    unsafe {
        Ok(std::slice::from_raw_parts(
            pybuffer.buf_ptr() as *const u8,
            pybuffer.len_bytes(),
        ))
    }
}

fn new_thread_pool(max_threads: usize) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new()
        .num_threads(max_threads)
        .build()
        .unwrap()
}

enum ThreadingMode {
    Single,
    Auto,
    Pool {
        pool: rayon::ThreadPool,
        max_threads: usize,
    },
}

impl Clone for ThreadingMode {
    fn clone(&self) -> Self {
        match self {
            ThreadingMode::Single => ThreadingMode::Single,
            ThreadingMode::Auto => ThreadingMode::Auto,
            ThreadingMode::Pool { max_threads, .. } => ThreadingMode::Pool {
                max_threads: *max_threads,
                pool: new_thread_pool(*max_threads),
            },
        }
    }
}

/// An incremental BLAKE3 hasher, which can accept any number of writes.
/// The interface is similar to `hashlib.blake2b` or `hashlib.md5` from the
/// standard library.
///
/// Arguments:
/// - `data`: Input bytes to hash. Setting this to non-None is equivalent
///   to calling `update` on the returned hasher.
/// - `key`: A 32-byte key. Setting this to non-None enables the BLAKE3
///   keyed hashing mode.
/// - `derive_key_context`: A hardcoded, globally unique,
///   application-specific context string. Setting this to non-None enables
///   the BLAKE3 key derivation mode. `derive_key_context` and `key` cannot
///   be used at the same time.
/// - `max_threads`: The maximum number of threads that the implementation
///   may use for hashing. The default value is 1, meaning single-threaded.
///   `max_threads` may be any positive integer, or the value of the class
///   attribute `blake3.AUTO`, which lets the implementation use as many
///   threads as it likes. (Currently this means a number of threads equal
///   to the number of logical CPU cores, but this is not guaranteed.) The
///   actual number of threads used may be less than the maximum and may
///   change over time. API-compatible reimplementations of this library
///   may also ignore this parameter entirely, if they don't support
///   multithreading.
/// - `usedforsecurity`: Currently ignored. See the standard hashlib docs.
// Note: The "blake3.blake3.blake3" canonical path is a Maturin implementation detail. See
// https://github.com/mkdocstrings/mkdocstrings/issues/451 for why we expose it here. That means
// that both of these work today, though most callers should prefer the first one:
//
//   # Import the re-exported blake3 class from the top-level module. This is stable. Do this.
//   from blake3 import blake3
//
//   # Import the blake3 class from its canonical path. Avoid this in regular code, because the
//   # canonical path is an internal implementation detail, and it could change in the future.
//   from blake3.blake3 import blake3
#[pyclass(name = "blake3", module = "blake3.blake3", frozen)]
struct Blake3Class {
    // By default (currently), PyO3 pyclass objects use an atomic RefCell-like
    // pattern to detect/prevent mutable aliasing. Rather than blocking, any
    // mutably aliasing accesses instead raises an exception. Normally the GIL
    // prevents this, but we release the GIL to hash long inputs, so ordinary
    // races between Python threads can trigger this. And freethreaded Python
    // builds have no GIL, so under those you can trigger this with any input
    // length.
    //
    // Now, we *could* decide that that's fine. Racing to update a hasher gives
    // you nondeterministic outputs, and it's hard to imagine a use case where
    // that's not a bug. However, someone using BLAKE3 as a CSPRNG might reseed
    // it periodically, and they might not care if other callers race with that.
    // Also, reliably raising exceptions is defensible, but *occasionally*
    // raising them is terrible. Users' tests will pass, and then their apps
    // will crash in prod.
    //
    // Instead, we declare this class "frozen" above, meaning that only shared
    // access is allowed. Then we use a Mutex internally to let us mutate the
    // Hasher. This means that users will never see exceptions about mutable
    // borrowing, and the PyO3 docs mention that they want to push the ecosystem
    // in this direction. See: https://pyo3.rs/main/class.html#frozen-classes-opting-out-of-interior-mutability
    rust_hasher: Mutex<upstream_blake3::Hasher>,
    threading_mode: ThreadingMode,
}

#[pymethods]
impl Blake3Class {
    /// The lowercase name of this hashing algorithm, "blake3".
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "blake3";

    /// The default size of the resulting hash in bytes, 32.
    #[classattr]
    #[allow(non_upper_case_globals)]
    const digest_size: usize = 32;

    /// The internal block size in bytes, 64.
    #[classattr]
    #[allow(non_upper_case_globals)]
    const block_size: usize = 64;

    /// The key size in bytes, 32.
    #[classattr]
    #[allow(non_upper_case_globals)]
    const key_size: usize = 32;

    /// Used as a `max_threads` value, to let the implementation choose the number of threads.
    ///
    /// This currently uses a number of threads equal to the number of logical cores, but that
    /// behavior could change in the future.
    #[classattr]
    const AUTO: isize = -1;

    #[new]
    #[pyo3(signature = (
        data = None,
        /,
        *,
        key = None,
        derive_key_context = None,
        max_threads = 1,
        usedforsecurity = true
    ))]
    fn new<'py>(
        py: Python<'py>,
        data: Option<&Bound<'py, PyAny>>,
        key: Option<&Bound<'py, PyAny>>,
        derive_key_context: Option<&str>,
        max_threads: isize,
        usedforsecurity: bool,
    ) -> PyResult<Blake3Class> {
        let _ = usedforsecurity; // currently ignored

        let mut rust_hasher = match (key, derive_key_context) {
            // The default, unkeyed hash function.
            (None, None) => upstream_blake3::Hasher::new(),
            // The keyed hash function.
            (Some(key_obj), None) => {
                // Use the same `as_bytes` helper function to get the key buffer
                // as `update` uses to get the data buffer. Even though we just
                // copy the bytes immediately here, technically this risks the
                // same race conditions.
                let key_buf = BytesPyBuffer::get(key_obj)?;
                let key_slice: &[u8] = unsafe { key_buf.as_bytes()? };
                let key_array: &[u8; 32] = if let Ok(array) = key_slice.try_into() {
                    array
                } else {
                    let msg = format!("expected a {}-byte key, found {}", 32, key_slice.len());
                    return Err(PyValueError::new_err(msg));
                };
                upstream_blake3::Hasher::new_keyed(key_array)
            }
            // The key derivation function.
            (None, Some(context)) => upstream_blake3::Hasher::new_derive_key(context),
            // Error: can't use both modes at the same time.
            (Some(_), Some(_)) => {
                return Err(PyValueError::new_err(
                    "cannot use key and derive_key_context at the same time",
                ));
            }
        };

        let threading_mode = match max_threads {
            1 => ThreadingMode::Single,
            Self::AUTO => ThreadingMode::Auto,
            n if n > 1 => ThreadingMode::Pool {
                max_threads: n as usize,
                pool: new_thread_pool(n as usize),
            },
            _ => return Err(PyValueError::new_err("not a valid number of threads")),
        };

        if let Some(data_obj) = data {
            // XXX: Get a &[u8] slice of the data bytes. The safety situation
            // here is complicated. See all the comments in bytes_from_pybuffer.
            let data_buf = BytesPyBuffer::get(data_obj)?;
            let data_slice: &[u8] = unsafe { data_buf.as_bytes()? };

            // Since rust_hasher isn't yet shared, we don't need to access it
            // through the Mutex here like we do in update() below.
            let mut update_closure = || match &threading_mode {
                ThreadingMode::Single => {
                    rust_hasher.update(data_slice);
                }
                ThreadingMode::Auto => {
                    rust_hasher.update_rayon(data_slice);
                }
                ThreadingMode::Pool { pool, .. } => pool.install(|| {
                    rust_hasher.update_rayon(data_slice);
                }),
            };

            if data_slice.len() >= GIL_MINSIZE {
                // Release the GIL while we hash this slice, so that we don't
                // block other threads. But again, see all the comments above
                // about data race risks.
                py.detach(update_closure);
            } else {
                // Don't bother releasing the GIL for short updates.
                update_closure();
            }
        }

        Ok(Blake3Class {
            rust_hasher: Mutex::new(rust_hasher),
            threading_mode,
        })
    }

    /// Add input bytes to the hasher. You can call this any number of
    /// times.
    ///
    /// Arguments:
    /// - `data` (required): The input bytes.
    #[pyo3(signature=(data, /))]
    fn update<'py>(
        this: Bound<'py, Self>,
        py: Python,
        data: &Bound<PyAny>,
    ) -> PyResult<Bound<'py, Self>> {
        let self_ = this.get();

        // XXX: Get a &[u8] slice of the data bytes. The safety situation here
        // is complicated. See all the comments in bytes_from_pybuffer.
        let data_buf = BytesPyBuffer::get(data)?;
        let data_slice: &[u8] = unsafe { data_buf.as_bytes()? };

        let update_closure = || match &self_.threading_mode {
            ThreadingMode::Single => {
                self_.rust_hasher.lock().unwrap().update(data_slice);
            }
            ThreadingMode::Auto => {
                self_.rust_hasher.lock().unwrap().update_rayon(data_slice);
            }
            ThreadingMode::Pool { pool, .. } => pool.install(|| {
                self_.rust_hasher.lock().unwrap().update_rayon(data_slice);
            }),
        };

        if data_slice.len() >= GIL_MINSIZE {
            // Release the GIL while we hash this slice, so that we don't
            // block other threads. But again, see all the comments above
            // about data race risks.
            py.detach(update_closure);
        } else {
            // Don't bother releasing the GIL for short updates.
            update_closure();
        }

        Ok(this)
    }

    /// Read a file using memory mapping and add its bytes to the hasher. You can call this any
    /// number of times.
    ///
    /// Arguments:
    /// - `path` (required): The filepath to read.
    #[pyo3(signature=(path))]
    fn update_mmap<'py>(
        this: Bound<'py, Self>,
        py: Python,
        path: PathBuf,
    ) -> PyResult<Bound<'py, Self>> {
        let self_ = this.get();

        py.detach(|| -> PyResult<()> {
            match &self_.threading_mode {
                ThreadingMode::Single => {
                    self_.rust_hasher.lock().unwrap().update_mmap(&path)?;
                }
                ThreadingMode::Auto => {
                    self_.rust_hasher.lock().unwrap().update_mmap_rayon(&path)?;
                }
                ThreadingMode::Pool { pool, .. } => {
                    pool.install(|| -> PyResult<()> {
                        self_.rust_hasher.lock().unwrap().update_mmap_rayon(&path)?;
                        Ok(())
                    })?;
                }
            }
            Ok(())
        })?;
        Ok(this)
    }

    /// Return a copy (“clone”) of the hasher. This can be used to
    /// efficiently compute the digests of data sharing a common initial
    /// substring.
    #[pyo3(signature=())]
    fn copy(&self) -> Blake3Class {
        Blake3Class {
            rust_hasher: Mutex::new(self.rust_hasher.lock().unwrap().clone()),
            threading_mode: self.threading_mode.clone(),
        }
    }

    /// Reset the hasher to its initial empty state. If the hasher contains
    /// an internal threadpool (as it currently does if `max_threads` is
    /// greater than 1), resetting the hasher lets you reuse that pool.
    /// Note that if any input bytes were supplied in the original
    /// construction of the hasher, those bytes are *not* replayed.
    #[pyo3(signature=())]
    fn reset(&self) {
        self.rust_hasher.lock().unwrap().reset();
    }

    /// Finalize the hasher and return the resulting hash as bytes. This
    /// does not modify the hasher, and calling it twice will give the same
    /// result. You can also add more input and finalize again.
    ///
    /// Arguments:
    /// - `length`: The number of bytes in the final hash. BLAKE3 supports
    ///   any output length up to 2**64-1. Note that shorter outputs are
    ///   prefixes of longer ones. Defaults to 32.
    /// - `seek`: The starting byte position in the output stream. Defaults
    ///   to 0.
    #[pyo3(signature=(length=32, *, seek=0))]
    fn digest<'p>(&self, py: Python<'p>, length: usize, seek: u64) -> PyResult<Bound<'p, PyBytes>> {
        if length > isize::MAX as usize {
            return Err(PyOverflowError::new_err("length overflows isize"));
        }
        let mut reader = self.rust_hasher.lock().unwrap().finalize_xof();
        reader.set_position(seek);
        PyBytes::new_with(py, length, |slice| {
            debug_assert_eq!(length, slice.len());
            if length >= GIL_MINSIZE {
                // This could be a long-running operation. Release the GIL.
                py.detach(|| reader.fill(slice));
            } else {
                // Don't bother releasing the GIL for short outputs.
                reader.fill(slice);
            }
            Ok(())
        })
    }

    /// Finalize the hasher and return the resulting hash as a hexadecimal
    /// string. This does not modify the hasher, and calling it twice will
    /// give the same result. You can also add more input and finalize
    /// again.
    ///
    /// Arguments:
    /// - `length`: The number of bytes in the final hash, prior to hex
    ///   encoding. BLAKE3 supports any output length up to 2**64-1. Note
    ///   that shorter outputs are prefixes of longer ones. Defaults to 32.
    /// - `seek`: The starting byte position in the output stream, prior to
    ///   hex encoding. Defaults to 0.
    #[pyo3(signature=(length=32, *, seek=0))]
    fn hexdigest<'p>(
        &self,
        py: Python<'p>,
        length: usize,
        seek: u64,
    ) -> PyResult<Bound<'p, PyString>> {
        if length > (isize::MAX / 2) as usize {
            return Err(PyOverflowError::new_err("length overflows isize"));
        }
        let bytes = self.digest(py, length, seek)?;
        let hex = hex::encode(bytes.as_bytes());
        Ok(PyString::new(py, &hex))
    }
}

/// Python bindings for the official Rust implementation of BLAKE3
/// (https://github.com/BLAKE3-team/BLAKE3). This module provides a single
/// class, also called `blake3.` The interface is similar to `hashlib` from
/// the standard library, which provides `blake2b`, `md5`, etc.
#[pymodule(gil_used = false)]
fn blake3(_: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<Blake3Class>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
