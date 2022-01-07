#define PY_SSIZE_T_CLEAN
#include <Python.h>

#include <stdbool.h>

#include "blake3.h"

// clang-format gets confused by the (correct, documented) missing semicolon
// after PyObject_HEAD.
// clang-format off
typedef struct {
  PyObject_HEAD
  blake3_hasher hasher;
} Blake3Object;
// clang-format on

static int Blake3_init(Blake3Object *self, PyObject *args, PyObject *kwds) {
  static char *kwlist[] = {
      "", // data, positional-only
      "key",
      "derive_key_context",
      "max_threads",     // currently ignored
      "usedforsecurity", // currently ignored
      NULL,
  };
  Py_buffer data;
  Py_buffer key;
  Py_buffer derive_key_context;
  Py_ssize_t max_threads;
  bool usedforsecurity;

  if (!PyArg_ParseTupleAndKeywords(args, kwds, "|OOi", kwlist, &data, &key,
                                   &derive_key_context, &max_threads,
                                   &usedforsecurity)) {
    return -1;
  }

  return 0;
}

// clang-format gets confused by the (correct, documented) missing semicolon
// after PyObject_HEAD_INIT.
// clang-format off
static PyTypeObject Blake3Type = {
    PyVarObject_HEAD_INIT(NULL, 0)
    .tp_name = "blake3",
    .tp_doc = "an incremental BLAKE3 hasher",
    .tp_basicsize = sizeof(Blake3Object),
    .tp_itemsize = 0,
    .tp_flags = Py_TPFLAGS_DEFAULT,
    .tp_new = PyType_GenericNew,
    .tp_init = (initproc) Blake3_init,
};
// clang-format on

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
  // Convert the output to a bytes object.
  PyObject *ret = Py_BuildValue("y#", output, BLAKE3_OUT_LEN);
  // The input buffer will be permanently locked if we don't release it.
  PyBuffer_Release(&input);
  return ret;
}

static PyMethodDef Blake3Methods[] = {
    {"hash", blake3_hash, METH_VARARGS, "Hash some bytes."},
    {NULL, NULL, 0, NULL} /* Sentinel */
};

static struct PyModuleDef blake3module = {
    PyModuleDef_HEAD_INIT,
    .m_name = "blake3",
    .m_doc = "experimental bindings for the BLAKE3 C implementation",
    .m_size = -1,
    .m_methods = Blake3Methods,
};

PyMODINIT_FUNC PyInit_blake3(void) {
  PyObject *m;
  if (PyType_Ready(&Blake3Type) < 0) {
    return NULL;
  }

  m = PyModule_Create(&blake3module);
  if (m == NULL) {
    return NULL;
  }

  // This refcount handling follows the example from
  // https://docs.python.org/3/extending/newtypes_tutorial.html.
  Py_INCREF(&Blake3Type);
  if (PyModule_AddObject(m, "blake3", (PyObject *)&Blake3Type) < 0) {
    Py_DECREF(&Blake3Type);
    Py_DECREF(m);
    return NULL;
  }

  return m;
}
