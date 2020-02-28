use pyo3::buffer::PyBuffer;
use pyo3::exceptions::TypeError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyString};
use pyo3::wrap_pyfunction;

// Obtain a slice of the bytes inside of `data` using the Python buffer
// protocol. (This supports e.g. bytes, bytearrays, and memoryviews.) Then
// release the GIL while we hash that slice. This matches the behavior of other
// hash implementations in the Python standard library.
fn hash_bytes_using_buffer_api(
    py: Python,
    hasher: &mut blake3::Hasher,
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
        hasher.update(slice);
    });

    // Explicitly release the buffer. This avoid re-acquiring the GIL in its
    // destructor.
    buffer.release(py);

    Ok(())
}

#[pymodule]
fn blake3(_: Python, m: &PyModule) -> PyResult<()> {
    // The hasher wrapper type is private. Similar to other types in hashlib,
    // it's only exposed through the `blake3()` constructor function.
    #[pyclass]
    struct Blake3Hasher {
        hasher: blake3::Hasher,
    }

    #[pymethods]
    impl Blake3Hasher {
        fn update(&mut self, py: Python, data: &PyAny) -> PyResult<()> {
            hash_bytes_using_buffer_api(py, &mut self.hasher, data)
        }

        fn digest<'p>(&self, py: Python<'p>) -> &'p PyBytes {
            PyBytes::new(py, self.hasher.finalize().as_bytes())
        }

        fn hexdigest<'p>(&self, py: Python<'p>) -> &'p PyString {
            PyString::new(py, &self.hasher.finalize().to_hex())
        }
    }

    #[pyfunction(data = "None")]
    fn blake3(py: Python, data: Option<&PyAny>) -> PyResult<Blake3Hasher> {
        let mut pyhasher = Blake3Hasher {
            hasher: blake3::Hasher::new(),
        };
        if let Some(data) = data {
            hash_bytes_using_buffer_api(py, &mut pyhasher.hasher, data)?;
        }
        Ok(pyhasher)
    }

    m.add_wrapped(wrap_pyfunction!(blake3))?;
    Ok(())
}
