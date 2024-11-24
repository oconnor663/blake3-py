extern crate blake3 as upstream_blake3;

use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{PyBufferError, PyOverflowError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyString};
use std::path::PathBuf;
use std::sync::Mutex;

// This is the same as HASHLIB_GIL_MINSIZE in CPython.
const GIL_MINSIZE: usize = 2048;

unsafe fn unsafe_slice_from_buffer<'a>(data: &'a Bound<PyAny>) -> PyResult<&'a [u8]> {
    // First see if we can get a u8 slice. This is the common case.
    match unsafe_slice_from_typed_buffer::<u8>(data) {
        // If that worked, return it.
        Ok(slice) => Ok(slice),
        // If not, then see if we can get an i8 buffer.
        Err(u8_err) => match unsafe_slice_from_typed_buffer::<i8>(data) {
            // That worked, and we've pointer-cast it to &[u8].
            Ok(slice) => Ok(slice),
            // That didn't work either. Return the first error from above,
            // because if they're different, the first one is more likely to be
            // relevant to the caller.
            Err(_i8_err) => Err(u8_err),
        },
    }
}

unsafe fn unsafe_slice_from_typed_buffer<'a, T: pyo3::buffer::Element>(
    data: &'a Bound<PyAny>,
) -> PyResult<&'a [u8]> {
    // Assert that we only ever try this for u8 and i8.
    assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<u8>());
    // If this object implements the buffer protocol for the element type we're
    // looking for, get a reference to that underlying buffer. We'll fail here
    // with a TypeError if `data` isn't a buffer at all.
    let pybuffer = PyBuffer::<T>::get(data)?;
    // Get a slice from the buffer. This fails if the buffer is not contiguous,
    // Regular bytes types are almost always contiguous, but things like NumPy
    // arrays can be "strided", and those will fail here.
    if let Some(readonly_slice) = pybuffer.as_slice(data.py()) {
        // We got a slice. For safety, PyO3 gives it to us as
        // &[ReadOnlyCell<T>], which is pretty much the same as a &[Cell<T>].
        // We're going to use unsafe code to cast that into a &[u8], which is
        // the only form blake3::Hasher::update will accept. This raises a few
        // risks:
        //
        // - We're potentially casting from &[i8] to &[u8]. I believe this is
        //   always allowed. There's a possibility that it could behave
        //   differently on (extremely rare) one's complement systems, compared
        //   to (typical) two's complement systems. However, I don't think Rust
        //   even supports one's complement systems, and also "reinterpret the
        //   bit pattern as unsigned" is likely to be what the caller expects
        //   anyway.
        // - This buffer might be aliased. This is the main reason why
        //   PyByteArray::as_bytes is unsafe and why PyO3 uses the ReadOnlyCell
        //   type. If we mutated any other buffers, or ran any unknown Python
        //   code that could do anything (including any object finalizer), we
        //   could end up mutating this buffer too. Luckily we don't do either
        //   of those things in this module.
        // - We're breaking the lifetime relationship between this slice and
        //   `py`, because we're going to release the GIL during update. That
        //   means *other threads* might mutate this buffer.
        //
        // The last point above is the most serious. Python locks buffers to
        // prevent resizing while we're looking at them, so we don't need to
        // worry about out-of-bounds reads or use-after-free here, but it's
        // still possible for another thread to write to the bytes of the buffer
        // while we're reading them. In practice, the result of a race here is
        // "probably just junk bytes", but technically this violates the
        // requirements of the Rust memory model, and there may be obscure
        // circumstances (now or in the future) where it does something worse.
        //
        // However, this isn't just our problem. The standard hash
        // implementations in Python's hashlib have the same behavior, and you
        // can trigger a real data race with standard types like this:
        // https://gist.github.com/oconnor663/c69cb4dbffb9b13bbced3fe8ce2181ac.
        // This data race violates the requirements of the C memory model also.
        //
        // At the end of the day, even if this race turns out to be exploitable
        // (which appears unlikely), only pathological programs should be able
        // to trigger it. Writing to a buffer concurrently from another thread
        // while hashing it is a very weird thing to do, and it's almost
        // guaranteed to be a correctness bug, regardless of whether it's also a
        // soundness bug.
        let readonly_ptr: *const pyo3::buffer::ReadOnlyCell<T> = readonly_slice.as_ptr();
        Ok(std::slice::from_raw_parts(
            readonly_ptr as *const u8,
            readonly_slice.len(),
        ))
    } else {
        // We couldn't get a slice, probably because this is a strided NumPy
        // array or something like that.
        Err(PyBufferError::new_err("buffer is not contiguous"))
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
#[pyclass(name = "blake3", module = "blake3.blake3")]
struct Blake3Class {
    // We release the GIL while updating this hasher, which means that other
    // threads could race to access it. Putting it in a Mutex keeps it safe.
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
    fn new(
        py: Python,
        data: Option<&Bound<PyAny>>,
        key: Option<&Bound<PyAny>>,
        derive_key_context: Option<&str>,
        max_threads: isize,
        usedforsecurity: bool,
    ) -> PyResult<Blake3Class> {
        let _ = usedforsecurity; // currently ignored

        let mut rust_hasher = match (key, derive_key_context) {
            // The default, unkeyed hash function.
            (None, None) => upstream_blake3::Hasher::new(),
            // The keyed hash function.
            (Some(key_buf), None) => {
                // Use the same function to get the key buffer as `update` uses
                // to get the data buffer. In this case it isn't for lifetime
                // reasons, but because we want to handle the buffer protocol in
                // the same way to support bytes/bytearray/memoryview etc. Even
                // though we just copy the bytes immediately, technically this
                // is the same race condition.
                let key_slice: &[u8] = unsafe { unsafe_slice_from_buffer(key_buf)? };
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
                ))
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

        if let Some(data) = data {
            // Get a slice that's not tied to the `py` lifetime.
            // XXX: The safety situation here is a bit complicated. See all the
            //      comments in unsafe_slice_from_buffer.
            let slice: &[u8] = unsafe { unsafe_slice_from_buffer(data)? };

            // Since rust_hasher isn't yet shared, we don't need to access it
            // through the Mutex here like we do in update() below.
            let mut update_closure = || match &threading_mode {
                ThreadingMode::Single => {
                    rust_hasher.update(slice);
                }
                ThreadingMode::Auto => {
                    rust_hasher.update_rayon(slice);
                }
                ThreadingMode::Pool { pool, .. } => pool.install(|| {
                    rust_hasher.update_rayon(slice);
                }),
            };

            if slice.len() >= GIL_MINSIZE {
                // Release the GIL while we hash this slice, so that we don't
                // block other threads. But again, see all the comments above
                // about data race risks.
                py.allow_threads(update_closure);
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
    fn update<'this>(
        mut this: PyRefMut<'this, Self>,
        py: Python,
        data: &Bound<PyAny>,
    ) -> PyResult<PyRefMut<'this, Self>> {
        // Get a slice that's not tied to the `py` lifetime.
        // XXX: The safety situation here is a bit complicated. See all the
        //      comments in unsafe_slice_from_buffer.
        let slice: &[u8] = unsafe { unsafe_slice_from_buffer(data)? };

        let this_mut = &mut *this;
        let mut update_closure = || match &mut this_mut.threading_mode {
            ThreadingMode::Single => {
                this_mut.rust_hasher.lock().unwrap().update(slice);
            }
            ThreadingMode::Auto => {
                this_mut.rust_hasher.lock().unwrap().update_rayon(slice);
            }
            ThreadingMode::Pool { pool, .. } => pool.install(|| {
                this_mut.rust_hasher.lock().unwrap().update_rayon(slice);
            }),
        };

        if slice.len() >= GIL_MINSIZE {
            // Release the GIL while we hash this slice, so that we don't
            // block other threads. But again, see all the comments above
            // about data race risks.
            py.allow_threads(update_closure);
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
    fn update_mmap<'this>(
        mut this: PyRefMut<'this, Self>,
        py: Python,
        path: PathBuf,
    ) -> PyResult<PyRefMut<'this, Self>> {
        let this_mut = &mut *this;
        py.allow_threads(|| -> PyResult<()> {
            match &mut this_mut.threading_mode {
                ThreadingMode::Single => {
                    this_mut.rust_hasher.lock().unwrap().update_mmap(&path)?;
                }
                ThreadingMode::Auto => {
                    this_mut
                        .rust_hasher
                        .lock()
                        .unwrap()
                        .update_mmap_rayon(&path)?;
                }
                ThreadingMode::Pool { pool, .. } => {
                    pool.install(|| -> PyResult<()> {
                        this_mut
                            .rust_hasher
                            .lock()
                            .unwrap()
                            .update_mmap_rayon(&path)?;
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
    fn reset(&mut self) {
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
        if length > isize::max_value() as usize {
            return Err(PyOverflowError::new_err("length overflows isize"));
        }
        let mut reader = self.rust_hasher.lock().unwrap().finalize_xof();
        reader.set_position(seek);
        PyBytes::new_with(py, length, |slice| {
            debug_assert_eq!(length, slice.len());
            if length >= GIL_MINSIZE {
                // This could be a long-running operation. Release the GIL.
                py.allow_threads(|| reader.fill(slice));
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
        if length > (isize::max_value() / 2) as usize {
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
#[pymodule]
fn blake3(_: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<Blake3Class>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
