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
    multithreading: bool,
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
        if multithreading {
            rust_hasher.update_with_join::<blake3::join::RayonJoin>(slice);
        } else {
            rust_hasher.update(slice);
        }
    });

    // Explicitly release the buffer. This avoid re-acquiring the GIL in its
    // destructor.
    buffer.release(py);

    Ok(())
}

fn output_bytes(rust_hasher: &blake3::Hasher, length: u64, seek: u64) -> PyResult<Vec<u8>> {
    if length > isize::max_value() as u64 {
        return Err(ValueError::py_err("length overflows isize"));
    }
    let mut reader = rust_hasher.finalize_xof();
    let mut output = vec![0; length as usize];
    reader.set_position(seek);
    reader.fill(&mut output);
    Ok(output)
}

/// Python bindings for the official Rust implementation of BLAKE3
/// (https://github.com/BLAKE3-team/BLAKE3). This module provides a single
/// function, also called `blake3.` The interface is similar to `hashlib` from
/// the standard library, which provides `blake2b`, `md5`, etc.
#[pymodule]
fn blake3(_: Python, m: &PyModule) -> PyResult<()> {
    // The hasher wrapper type is private. Similar to other types in hashlib,
    // it's only exposed through the `blake3()` constructor function.
    /// An incremental BLAKE3 hasher.
    #[pyclass]
    struct Blake3Hasher {
        rust_hasher: blake3::Hasher,
    }

    #[pymethods]
    impl Blake3Hasher {
        /// Add input bytes to the hasher. You can call this any number of
        /// times.
        ///
        /// Positional arguments:
        /// - `data` (required): The input bytes.
        ///
        /// Keyword arguments:
        /// - `multithreading`: Setting this to True enables Rayon-based
        ///   mulithreading in the underlying Rust implementation. This can be a
        ///   large performance improvement for long inputs, but it can also hurt
        ///   performance for short inputs. As a rule of thumb, multithreading
        ///   works well on inputs that are larger than 1 MB. It's a good idea to
        ///   benchmark this to see if it helps your use case.
        fn update(
            &mut self,
            py: Python,
            data: &PyAny,
            multithreading: Option<bool>,
        ) -> PyResult<()> {
            hash_bytes_using_buffer_api(
                py,
                &mut self.rust_hasher,
                data,
                multithreading.unwrap_or(false),
            )
        }

        /// Finalize the hasher and return the resulting hash as bytes. This
        /// does not modify the hasher, and calling it twice will give the same
        /// result. You can also add more input and finalize again.
        ///
        /// Keyword arguments:
        /// - `length`: The number of bytes in the final hash. BLAKE3 supports
        ///   any output length up to 2**64-1. Note that shorter outputs are
        ///   prefixes of longer ones. Defaults to 32.
        /// - `seek`: The starting byte position in the output stream. Defaults
        ///   to 0.
        fn digest<'p>(
            &self,
            py: Python<'p>,
            length: Option<u64>,
            seek: Option<u64>,
        ) -> PyResult<&'p PyBytes> {
            let bytes = output_bytes(
                &self.rust_hasher,
                length.unwrap_or(blake3::KEY_LEN as u64),
                seek.unwrap_or(0),
            )?;
            Ok(PyBytes::new(py, &bytes))
        }

        /// Finalize the hasher and return the resulting hash as a hexadecimal
        /// string. This does not modify the hasher, and calling it twice will
        /// give the same result. You can also add more input and finalize
        /// again.
        ///
        /// Keyword arguments:
        /// - `length`: The number of bytes in the final hash, prior to hex
        ///   encoding. BLAKE3 supports any output length up to 2**64-1. Note
        ///   that shorter outputs are prefixes of longer ones. Defaults to 32.
        /// - `seek`: The starting byte position in the output stream, prior to
        ///   hex encoding. Defaults to 0.
        fn hexdigest<'p>(
            &self,
            py: Python<'p>,
            length: Option<u64>,
            seek: Option<u64>,
        ) -> PyResult<&'p PyString> {
            let bytes = output_bytes(
                &self.rust_hasher,
                length.unwrap_or(blake3::KEY_LEN as u64),
                seek.unwrap_or(0),
            )?;
            let hex = hex::encode(&bytes);
            Ok(PyString::new(py, &hex))
        }
    }

    /// Construct an incremental hasher object, which can accept any number of
    /// writes. The interface is similar to `hashlib.blake2b` or `hashlib.md5`
    /// from the standard library.
    ///
    /// Positional arguments:
    /// - `data` (optional): Input bytes to hash. This is equivalent to calling
    ///   `update` on the returned hasher.
    ///
    /// Keyword arguments:
    /// - `key`: A 32-byte key. Setting this to non-None enables the keyed
    ///   hashing mode.
    /// - `context`: A context string. Setting this to non-None enables the key
    ///   derivation mode. Context strings should be hardcoded, globally
    ///   unique, and application-specific. `context` and `key` cannot be used
    ///   at the same time.
    /// - `multithreading`: Setting this to True enables Rayon-based
    ///   mulithreading in the underlying Rust implementation. This can be a
    ///   large performance improvement for long inputs, but it can also hurt
    ///   performance for short inputs. As a rule of thumb, multithreading
    ///   works well on inputs that are larger than 1 MB. It's a good idea to
    ///   benchmark this to see if it helps your use case.
    #[pyfunction]
    fn blake3(
        py: Python,
        data: Option<&PyAny>,
        key: Option<&[u8]>,
        context: Option<&str>,
        multithreading: Option<bool>,
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
            hash_bytes_using_buffer_api(
                py,
                &mut rust_hasher,
                data,
                multithreading.unwrap_or(false),
            )?;
        }
        Ok(Blake3Hasher { rust_hasher })
    }

    m.add_wrapped(wrap_pyfunction!(blake3))?;
    m.add("OUT_LEN", blake3::OUT_LEN)?;
    m.add("KEY_LEN", blake3::KEY_LEN)?;
    Ok(())
}
