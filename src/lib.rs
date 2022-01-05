use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{PyBufferError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyString};

unsafe fn unsafe_slice_from_buffer<'a>(py: Python, data: &'a PyAny) -> PyResult<&'a [u8]> {
    // First see if we can get a u8 slice. This is the common case.
    match unsafe_slice_from_typed_buffer::<u8>(py, data) {
        // If that worked, return it.
        Ok(slice) => Ok(slice),
        // If not, then see if we can get an i8 buffer.
        Err(u8_err) => match unsafe_slice_from_typed_buffer::<i8>(py, data) {
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
    py: Python,
    data: &'a PyAny,
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
    if let Some(readonly_slice) = pybuffer.as_slice(py) {
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
        // - This buffer might be aliased by other Python values. This is the
        //   reason PyByteArray::as_bytes is unsafe, and the reason PyO3 uses
        //   the ReadOnlyCell type. This isn't an issue for us here, though,
        //   because we're not dealing with any values other than `data`, we're
        //   not calling into any other Python code, and we're not mutating
        //   anything ourselves.
        // - We're breaking the lifetime relationship between this slice and
        //   `py`, because we're going to release the GIL during update.
        //
        // The last point above is the most serious. If we were retaining the
        // GIL, we could reason that no other thread could do something awful
        // like resizing the buffer while we're reading it. (Python appears to
        // raise something like "BufferError: Existing exports of data: object
        // cannot be re-sized" in that case, but I don't know if we can rely on
        // that as a safety guarantee, and in any case other threads can at
        // least write to the buffer.) However, retaining the GIL during update
        // is an unacceptable performance issue, because update is potentially
        // long-running. If we retained the GIL, then an app hashing a large
        // buffer on a background thread might inadvertently block its main
        // thread from processing UI events for a second or more.
        //
        // The standard hashing implementations in Python's hashlib have the
        // same problem. They release the GIL too. You can trigger a real data
        // race with standard types like this:
        // https://gist.github.com/oconnor663/c69cb4dbffb9b13bbced3fe8ce2181ac
        //
        // At the end of the day, the situation appears to be this:
        //
        // - Even if this race turns out to be exploitable, in practice only
        //   pathological programs should trigger it. Updating a hasher
        //   concurrently from different threads is just a weird thing to do,
        //   and it's almost always a correctness bug, regardless of whether
        //   it's a soundness/security bug too.
        // - This sort of data race risk seems to be typical when Python
        //   extensions release the GIL to call into long-running native code.
        //   Again, this is what hashlib does too.
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

fn output_bytes(rust_hasher: &blake3::Hasher, length: u64, seek: u64) -> PyResult<Vec<u8>> {
    if length > isize::max_value() as u64 {
        return Err(PyValueError::new_err("length overflows isize"));
    }
    let mut reader = rust_hasher.finalize_xof();
    let mut output = vec![0; length as usize];
    reader.set_position(seek);
    reader.fill(&mut output);
    Ok(output)
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
#[pyclass(name = "blake3")]
#[pyo3(
    text_signature = "(data=b'', /, *, key=None, derive_key_context=None, max_threads=1, usedforsecurity=True)"
)]
#[derive(Clone)]
struct Blake3Class {
    rust_hasher: blake3::Hasher,
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

    /// TODO
    #[classattr]
    const AUTO: isize = -1;

    #[new]
    #[args(
        data,
        "/",
        "*",
        key = "None",
        derive_key_context = "None",
        max_threads = "1",
        usedforsecurity = "true"
    )]
    fn new(
        py: Python,
        data: Option<&PyAny>,
        key: Option<&PyAny>,
        derive_key_context: Option<&str>,
        max_threads: isize,
        usedforsecurity: bool,
    ) -> PyResult<Blake3Class> {
        let _ = usedforsecurity; // currently ignored
        let rust_hasher = match (key, derive_key_context) {
            // The default, unkeyed hash function.
            (None, None) => blake3::Hasher::new(),
            // The keyed hash function.
            (Some(key_buf), None) => {
                // Use the same function to get the key buffer as `update` uses
                // to get the data buffer. In this case it isn't for lifetime
                // reasons, but because we want to handle the buffer protocol in
                // the same way to support bytes/bytearray/memoryview etc.
                // We're going to copy the bytes immediately, so we don't have
                // the same race condition issues here.
                let key_slice: &[u8] = unsafe { unsafe_slice_from_buffer(py, key_buf)? };
                let key_array: &[u8; 32] = if let Ok(array) = key_slice.try_into() {
                    array
                } else {
                    let msg = format!("expected a {}-byte key, found {}", 32, key_slice.len());
                    return Err(PyValueError::new_err(msg));
                };
                blake3::Hasher::new_keyed(key_array)
            }
            // The key derivation function.
            (None, Some(context)) => blake3::Hasher::new_derive_key(context),
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
        let mut python_hasher = Blake3Class {
            rust_hasher,
            threading_mode,
        };
        if let Some(data) = data {
            python_hasher.update(py, data)?;
        }
        Ok(python_hasher)
    }

    /// Add input bytes to the hasher. You can call this any number of
    /// times.
    ///
    /// Note that `update` is not thread safe, and multiple threads sharing
    /// a single instance must use a `threading.Lock` or similar if one of
    /// them might be calling `update`. The thread safety issues here are
    /// worse than usual, because this method releases the GIL internally.
    /// However, updating one hasher from multiple threads is a very odd
    /// thing to do, and real world program almost never need to worry about
    /// this.
    ///
    /// Arguments:
    /// - `data` (required): The input bytes.
    #[args(data, "/")]
    #[pyo3(text_signature = "(data, /)")]
    fn update(&mut self, py: Python, data: &PyAny) -> PyResult<()> {
        // Get a slice that's not tied to the `py` lifetime.
        // XXX: The safety situation here is a bit complicated. See all the
        //      comments in unsafe_slice_from_buffer.
        let slice: &[u8] = unsafe { unsafe_slice_from_buffer(py, data)? };

        // Release the GIL while we hash the slice. This ensures that we
        // won't block other threads if this update is long running. But
        // again, see all the comments above about data race risks.
        py.allow_threads(|| match &mut self.threading_mode {
            ThreadingMode::Single => {
                self.rust_hasher.update(slice);
            }
            ThreadingMode::Auto => {
                self.rust_hasher.update_rayon(slice);
            }
            ThreadingMode::Pool { pool, .. } => {
                pool.install(|| self.rust_hasher.update_rayon(slice));
            }
        });
        Ok(())
    }

    /// Return a copy (“clone”) of the hasher. This can be used to
    /// efficiently compute the digests of data sharing a common initial
    /// substring.
    ///
    /// This is a read-only method, but the multithreading warning attached
    /// to the `update` method applies here. It is not safe to call this
    /// method while another thread might be calling `update` on the same
    /// instance.
    #[args()]
    #[pyo3(text_signature = "()")]
    fn copy(&self) -> Blake3Class {
        self.clone()
    }

    /// Reset the hasher to its initial empty state. If the hasher contains
    /// an internal threadpool (as it currently does if `max_threads` is
    /// greater than 1), resetting the hasher lets you reuse that pool.
    /// Note that if any input bytes were supplied in the original
    /// construction of the hasher, those bytes are *not* replayed.
    ///
    /// The multithreading warning attached to the `update` method applies
    /// here. It is not safe to call this method while another thread might
    /// be calling `update` on the same instance.
    #[args()]
    #[pyo3(text_signature = "()")]
    fn reset(&mut self) {
        self.rust_hasher.reset();
    }

    /// Finalize the hasher and return the resulting hash as bytes. This
    /// does not modify the hasher, and calling it twice will give the same
    /// result. You can also add more input and finalize again.
    ///
    /// This is a read-only method, but the multithreading warning attached
    /// to the `update` method applies here. It is not safe to call this
    /// method while another thread might be calling `update` on the same
    /// instance.
    ///
    /// Arguments:
    /// - `length`: The number of bytes in the final hash. BLAKE3 supports
    ///   any output length up to 2**64-1. Note that shorter outputs are
    ///   prefixes of longer ones. Defaults to 32.
    /// - `seek`: The starting byte position in the output stream. Defaults
    ///   to 0.
    #[args(length = "32", "*", seek = "0")]
    #[pyo3(text_signature = "(length=32, *, seek=0)")]
    fn digest<'p>(&self, py: Python<'p>, length: u64, seek: u64) -> PyResult<&'p PyBytes> {
        let bytes = output_bytes(&self.rust_hasher, length, seek)?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// Finalize the hasher and return the resulting hash as a hexadecimal
    /// string. This does not modify the hasher, and calling it twice will
    /// give the same result. You can also add more input and finalize
    /// again.
    ///
    /// This is a read-only method, but the multithreading warning attached
    /// to the `update` method applies here. It is not safe to call this
    /// method while another thread might be calling `update` on the same
    /// instance.
    ///
    /// Arguments:
    /// - `length`: The number of bytes in the final hash, prior to hex
    ///   encoding. BLAKE3 supports any output length up to 2**64-1. Note
    ///   that shorter outputs are prefixes of longer ones. Defaults to 32.
    /// - `seek`: The starting byte position in the output stream, prior to
    ///   hex encoding. Defaults to 0.
    #[args(length = "32", "*", seek = "0")]
    #[pyo3(text_signature = "(length=32, *, seek=0)")]
    fn hexdigest<'p>(&self, py: Python<'p>, length: u64, seek: u64) -> PyResult<&'p PyString> {
        let bytes = output_bytes(&self.rust_hasher, length, seek)?;
        let hex = hex::encode(&bytes);
        Ok(PyString::new(py, &hex))
    }
}

/// Python bindings for the official Rust implementation of BLAKE3
/// (https://github.com/BLAKE3-team/BLAKE3). This module provides a single
/// class, also called `blake3.` The interface is similar to `hashlib` from
/// the standard library, which provides `blake2b`, `md5`, etc.
#[pymodule]
fn blake3(_: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Blake3Class>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
