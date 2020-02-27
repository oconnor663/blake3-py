use pyo3::prelude::*;
use pyo3::wrap_pyfunction;
use pyo3::types::{PyBytes, PyString};

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
        fn update(&mut self, data: &[u8]) {
            self.hasher.update(data);
        }

        fn digest<'p>(&self, py: Python<'p>) -> &'p PyBytes {
            PyBytes::new(py, self.hasher.finalize().as_bytes())
        }

        fn hexdigest<'p>(&self, py: Python<'p>) -> &'p PyString {
            PyString::new(py, &self.hasher.finalize().to_hex())
        }
    }

    #[pyfunction(data="&[][..]")]
    fn blake3(data: &[u8]) -> Blake3Hasher {
        let mut hasher = Blake3Hasher {
            hasher: blake3::Hasher::new()
        };
        hasher.update(data);
        hasher
    }

    m.add_wrapped(wrap_pyfunction!(blake3))?;
    Ok(())
}
