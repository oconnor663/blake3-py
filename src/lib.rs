use pyo3::prelude::*;
use pyo3::wrap_pyfunction;

#[pymodule]
fn blake3(_: Python, m: &PyModule) -> PyResult<()> {
    #[pyfunction]
    fn blake3() -> i32 {
        42
    }

    m.add_wrapped(wrap_pyfunction!(blake3))?;
    Ok(())
}
