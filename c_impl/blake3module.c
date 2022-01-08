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

static void release_if_acquired(Py_buffer *buf) {
  if (buf != NULL && buf->obj != NULL) {
    PyBuffer_Release(buf);
  }
}

static bool weird_buffer(Py_buffer *buf) {
  if (buf != NULL && buf->obj != NULL && buf->itemsize != 1) {
    PyErr_SetString(PyExc_ValueError, "buffer elements must be bytes");
    return true;
  }
  return false;
}

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
    goto exit;
  }

  if (weird_buffer(&data)) {
    goto exit;
  }

  if (weird_buffer(&key)) {
    goto exit;
  }

  if (key.obj != NULL && derive_key_context != NULL) {
    PyErr_SetString(PyExc_ValueError,
                    "key and derive_key_context can't be used together");
    goto exit;
  }

  if (key.obj != NULL && key.len != BLAKE3_KEY_LEN) {
    PyErr_SetString(PyExc_ValueError, "keys must be 32 bytes");
    goto exit;
  }

  if (max_threads < 1 && max_threads != AUTO) {
    PyErr_SetString(PyExc_ValueError, "invalid value for max_threads");
    goto exit;
  }

  if (key.obj != NULL) {
    blake3_hasher_init_keyed(&self->hasher, key.buf);
  } else if (derive_key_context != NULL) {
    blake3_hasher_init_derive_key(&self->hasher, derive_key_context);
  } else {
    blake3_hasher_init(&self->hasher);
  }

  if (data.obj != NULL) {
    blake3_hasher_update(&self->hasher, data.buf, data.len);
  }

  // success
  ret = 0;

exit:
  release_if_acquired(&data);
  release_if_acquired(&key);
  return ret;
}

static PyObject *Blake3_update(Blake3Object *self, PyObject *args) {
  Py_buffer data = {0};

  PyObject *ret = NULL;

  if (!PyArg_ParseTuple(args, "y*", &data)) {
    goto exit;
  }

  if (weird_buffer(&data)) {
    goto exit;
  }

  blake3_hasher_update(&self->hasher, data.buf, data.len);

  // success
  Py_INCREF(Py_None);
  ret = Py_None;

exit:
  release_if_acquired(&data);
  return ret;
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
  PyObject *attr_dict = NULL;
  PyObject *name_str = NULL;
  PyObject *block_size_int = NULL;
  PyObject *digest_size_int = NULL;
  PyObject *key_size_int = NULL;
  PyObject *auto_int = NULL;
  PyObject *module = NULL;

  PyObject *ret = NULL;

  // Feedback needed: Handling all these possible allocation failures is
  // annoying and error-prone. Is this really necessary?
  attr_dict = PyDict_New();
  if (attr_dict == NULL) {
    goto exit;
  }
  name_str = PyUnicode_FromString("blake3");
  if (name_str == NULL) {
    goto exit;
  }
  if (PyDict_SetItemString(attr_dict, "name", name_str) < 0) {
    goto exit;
  }
  block_size_int = PyLong_FromLong(BLAKE3_BLOCK_LEN);
  if (block_size_int == NULL) {
    goto exit;
  }
  if (PyDict_SetItemString(attr_dict, "block_size", block_size_int) < 0) {
    goto exit;
  }
  digest_size_int = PyLong_FromLong(BLAKE3_OUT_LEN);
  if (digest_size_int == NULL) {
    goto exit;
  }
  if (PyDict_SetItemString(attr_dict, "digest_size", digest_size_int) < 0) {
    goto exit;
  }
  key_size_int = PyLong_FromLong(BLAKE3_KEY_LEN);
  if (key_size_int == NULL) {
    goto exit;
  }
  if (PyDict_SetItemString(attr_dict, "key_size", key_size_int) < 0) {
    goto exit;
  }
  auto_int = PyLong_FromLong(AUTO);
  if (auto_int == NULL) {
    goto exit;
  }
  if (PyDict_SetItemString(attr_dict, "AUTO", auto_int) < 0) {
    goto exit;
  }

  Blake3Type.tp_dict = attr_dict;
  attr_dict = NULL; // pass the refcount

  if (PyType_Ready(&Blake3Type) < 0) {
    goto exit;
  }

  module = PyModule_Create(&blake3module);
  if (module == NULL) {
    goto exit;
  }

  if (PyModule_AddObjectRef(module, "blake3", (PyObject *)&Blake3Type) < 0) {
    goto exit;
  }

  if (PyModule_AddStringConstant(module, "__version__", "0.0.0") < 0) {
    goto exit;
  }

  // success
  ret = module;
  module = NULL; // pass the refcount

exit:
  Py_XDECREF(attr_dict);
  Py_XDECREF(name_str);
  Py_XDECREF(block_size_int);
  Py_XDECREF(digest_size_int);
  Py_XDECREF(key_size_int);
  Py_XDECREF(auto_int);
  Py_XDECREF(module);
  return ret;
}
