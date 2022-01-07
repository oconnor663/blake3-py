#define PY_SSIZE_T_CLEAN
#include <Python.h>

#include <stdbool.h>

#include "blake3.h"

#define AUTO -1

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
  Py_buffer data = {0};
  Py_buffer key = {0};
  const char *derive_key_context = NULL;
  Py_ssize_t max_threads = 1;
  bool usedforsecurity = true;

  int ret = -1;

  if (!PyArg_ParseTupleAndKeywords(args, kwds, "|y*$y*snp", kwlist, &data, &key,
                                   &derive_key_context, &max_threads,
                                   &usedforsecurity)) {
    // nothing to release
    return ret;
  }

  if (key.buf != NULL && derive_key_context != NULL) {
    PyErr_SetString(PyExc_ValueError,
                    "key and derive_key_context can't be used together");
    goto exit;
  }

  if (key.buf != NULL && key.len != BLAKE3_KEY_LEN) {
    PyErr_SetString(PyExc_ValueError, "keys must be 32 bytes");
    goto exit;
  }

  if (max_threads < 1 && max_threads != AUTO) {
    PyErr_SetString(PyExc_ValueError, "invalid value for max_threads");
    goto exit;
  }

  if (key.buf != NULL) {
    blake3_hasher_init_keyed(&self->hasher, key.buf);
  } else if (derive_key_context != NULL) {
    blake3_hasher_init_derive_key(&self->hasher, derive_key_context);
  } else {
    blake3_hasher_init(&self->hasher);
  }

  if (data.buf != NULL) {
    blake3_hasher_update(&self->hasher, data.buf, data.len);
  }

  // success
  ret = 0;

exit:
  if (data.buf != NULL) {
    PyBuffer_Release(&data);
  }
  if (key.buf != NULL) {
    PyBuffer_Release(&key);
  }
  return ret;
}

static PyObject *Blake3_update(Blake3Object *self, PyObject *args) {
  Py_buffer data = {0};
  if (!PyArg_ParseTuple(args, "y*", &data)) {
    return NULL;
  }
  blake3_hasher_update(&self->hasher, data.buf, data.len);
  PyBuffer_Release(&data);
  Py_RETURN_NONE;
}

static PyObject *Blake3_digest(Blake3Object *self, PyObject *args,
                               PyObject *kwds) {
  static char *kwlist[] = {
      "length",
      "seek",
      NULL,
  };
  Py_ssize_t length = BLAKE3_OUT_LEN;
  unsigned long long seek = 0;
  if (!PyArg_ParseTupleAndKeywords(args, kwds, "|n$K", kwlist, &length,
                                   &seek)) {
    return NULL;
  }
  // Create a bytes object as per https://stackoverflow.com/a/55876332/823869.
  PyObject *output = PyBytes_FromStringAndSize(NULL, length);
  if (output == NULL) {
    return NULL;
  }
  blake3_hasher_finalize_seek(&self->hasher, seek,
                              (uint8_t *)PyBytes_AsString(output), length);
  return output;
}

static PyObject *Blake3_hexdigest(Blake3Object *self, PyObject *args,
                                  PyObject *kwds) {
  PyObject *bytes = Blake3_digest(self, args, kwds);
  if (bytes == NULL) {
    return NULL;
  }
  PyObject *hex = PyObject_CallMethod(bytes, "hex", NULL);
  Py_DECREF(bytes);
  return hex;
}

// Implemented below, because it needs to refer to Blake3Type.
static PyObject *Blake3_copy(Blake3Object *self, PyObject *args);

static PyObject *Blake3_reset(Blake3Object *self, PyObject *args) {
  blake3_hasher_reset(&self->hasher);
  Py_RETURN_NONE;
}

static PyMethodDef Blake3_methods[] = {
    {"update", (PyCFunction)Blake3_update, METH_VARARGS, "add input bytes"},
    {"digest", (PyCFunction)Blake3_digest, METH_VARARGS | METH_KEYWORDS,
     "finalize the hash"},
    {"hexdigest", (PyCFunction)Blake3_hexdigest, METH_VARARGS | METH_KEYWORDS,
     "finalize the hash and encode the result as hex"},
    {"update", (PyCFunction)Blake3_update, METH_VARARGS, "add input bytes"},
    {"copy", (PyCFunction)Blake3_copy, METH_VARARGS,
     "make a copy of this hasher"},
    {"reset", (PyCFunction)Blake3_reset, METH_VARARGS,
     "reset this hasher to its initial state"},
    {NULL, NULL, 0, NULL} // sentinel
};

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
    .tp_methods = Blake3_methods,
};
// clang-format on

// Declared above but implemented here, because it needs to refer to Blake3Type.
static PyObject *Blake3_copy(Blake3Object *self, PyObject *args) {
  Blake3Object *copy = PyObject_New(Blake3Object, &Blake3Type);
  if (copy == NULL) {
    return NULL;
  }
  memcpy(&copy->hasher, &self->hasher, sizeof(blake3_hasher));
  return (PyObject *)copy;
}

static struct PyModuleDef blake3module = {
    PyModuleDef_HEAD_INIT,
    .m_name = "blake3",
    .m_doc = "experimental bindings for the BLAKE3 C implementation",
    .m_size = -1,
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
