#define PY_SSIZE_T_CLEAN
#include <Python.h>

#include "blake3.h"

static PyObject *blake3_hash(PyObject *self, PyObject *args) {
  Py_buffer input;
  if (!PyArg_ParseTuple(args, "y*", &input)) {
    return NULL;
  }
  uint8_t output[BLAKE3_OUT_LEN];
  blake3_hasher hasher;
  blake3_hasher_init(&hasher);
  blake3_hasher_update(&hasher, input.buf, input.len);
  blake3_hasher_finalize(&hasher, output, BLAKE3_OUT_LEN);
  // Convert the output to a bytes object and return it.
  return Py_BuildValue("y#", output, BLAKE3_OUT_LEN);
}

static PyMethodDef Blake3Methods[] = {
    {"hash", blake3_hash, METH_VARARGS, "Hash some bytes."},
    {NULL, NULL, 0, NULL} /* Sentinel */
};

static struct PyModuleDef blake3module = {
    PyModuleDef_HEAD_INIT, // standard header
    "blake3",              // name of module
    NULL,                  // module documentation, may be NULL
    -1, // size of per-interpreter state of the module, or -1 if the module
        // keeps state in global variables.
    Blake3Methods,
};

PyMODINIT_FUNC PyInit_blake3(void) { return PyModule_Create(&blake3module); }
