use arrayref::array_ref;
use blake3::KEY_LEN;
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{TypeError, ValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyString};
use pyo3::wrap_pyfunction;

// Obtain a slice of the bytes inside of `data` using the Python buffer
// protocol. (This supports e.g. bytes, bytearrays, and memoryviews.) Then
// release the GIL while we hash that slice. This matches the behavior of other
// hash implementations in the Python standard library.
fn hash_bytes_using_buffer_api(
    py: Python,
    rust_hasher: &mut blake3::Hasher,
    data: &PyAny,
) -> PyResult<()> {
    let buffer = PyBuffer::get(py, data)?;

    // Check that the buffer is "simple".
    // XXX: Are these checks sufficient?
    if buffer.item_size() != 1 {
        return Err(TypeError::py_err("buffer must contain bytes"));
    }
    if buffer.dimensions() != 1 {
        return Err(TypeError::py_err("buffer must be 1-dimensional"));
    }
    if !buffer.is_c_contiguous() {
        return Err(TypeError::py_err("buffer must be contiguous"));
    }

    // Having verified that the buffer contains plain bytes, construct a slice
    // of its contents. Assuming the checks above are sufficient, I believe
    // this is sound. However, things gets trickier when we release the GIL
    // immediately below.
    let slice =
        unsafe { std::slice::from_raw_parts(buffer.buf_ptr() as *const u8, buffer.item_count()) };

    // Release the GIL while we hash the slice.
    // XXX: This is per se unsound. Another Python thread with a reference to
    // `data` could write to it while this slice exists, which violates Rust's
    // aliasing rules. It's possible this could result in "just getting a racy
    // answer", but I'm not sure. Here's an example of triggering the same race
    // in pure Python: https://gist.github.com/oconnor663/c69cb4dbffb9b13bbced3fe8ce2181ac
    py.allow_threads(|| {
        rust_hasher.update(slice);
    });

    // Explicitly release the buffer. This avoid re-acquiring the GIL in its
    // destructor.
    buffer.release(py);

    Ok(())
}

/// Python bindings for the Rust `blake3` crate. This module provides a single
/// function, also called `blake3.` This interface is similar to `hashlib` from
/// the standard library.
#[pymodule]
fn blake3(_: Python, m: &PyModule) -> PyResult<()> {
    // The hasher wrapper type is private. Similar to other types in hashlib,
    // it's only exposed through the `blake3()` constructor function.
    #[pyclass]
    struct Blake3Hasher {
        rust_hasher: blake3::Hasher,
    }

    #[pymethods]
    impl Blake3Hasher {
        /// Add input bytes to the hasher. You can call this any number of
        /// times.
        fn update(&mut self, py: Python, data: &PyAny) -> PyResult<()> {
            hash_bytes_using_buffer_api(py, &mut self.rust_hasher, data)
        }

        /// Finalize the hasher and return the resulting hash as bytes. This
        /// does not modify the hasher, and calling it twice will give the same
        /// result. You can also add more input and finalize again.
        fn digest<'p>(&self, py: Python<'p>) -> &'p PyBytes {
            PyBytes::new(py, self.rust_hasher.finalize().as_bytes())
        }

        /// Finalize the hasher and return the resulting hash as a hexadecimal
        /// string. This does not modify the hasher, and calling it twice will
        /// give the same result. You can also add more input and finalize
        /// again.
        fn hexdigest<'p>(&self, py: Python<'p>) -> &'p PyString {
            PyString::new(py, &self.rust_hasher.finalize().to_hex())
        }
    }

    /// Construct an incremental hasher object, which can accept any number of
    /// writes. This interface is similar to `hashlib.blake2b` or `hashlib.md5`
    /// from the standard library. The optional `data` argument also accepts
    /// bytes to hash, equivalent to a call to `update`.
    #[pyfunction(data = "None", key = "None")]
    fn blake3(
        py: Python,
        data: Option<&PyAny>,
        key: Option<&[u8]>,
        context: Option<&str>,
    ) -> PyResult<Blake3Hasher> {
        let mut rust_hasher = match (key, context) {
            // The default, unkeyed hash function.
            (None, None) => blake3::Hasher::new(),
            // The keyed hash function.
            (Some(key), None) => {
                if key.len() == KEY_LEN {
                    blake3::Hasher::new_keyed(array_ref!(key, 0, KEY_LEN))
                } else {
                    return Err(ValueError::py_err(format!(
                        "expected a {}-byte key, found {}",
                        KEY_LEN,
                        key.len()
                    )));
                }
            }
            // The key derivation function.
            (None, Some(context)) => blake3::Hasher::new_derive_key(context),
            // Error: can't use both modes at the same time.
            (Some(_), Some(_)) => {
                return Err(ValueError::py_err(
                    "cannot use key and context at the same time",
                ))
            }
        };
        if let Some(data) = data {
            hash_bytes_using_buffer_api(py, &mut rust_hasher, data)?;
        }
        Ok(Blake3Hasher { rust_hasher })
    }

    m.add_wrapped(wrap_pyfunction!(blake3))?;
    Ok(())
}
