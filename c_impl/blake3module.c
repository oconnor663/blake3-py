#define PY_SSIZE_T_CLEAN
#include <Python.h>

#include <stdbool.h>
#include <stdio.h>

#include "blake3.h"

#define AUTO -1

#define BUFSIZE 65536

// CPython defines HASHLIB_GIL_MINSIZE in hashlib.h. We'll want to remove this
// definition if this code is added to CPython.
#ifdef HASHLIB_GIL_MINSIZE
#error Already defined. Delete these lines?
#else
#define HASHLIB_GIL_MINSIZE 2048
#endif

static void release_buf_if_acquired(Py_buffer *buf) {
  if (buf != NULL && buf->obj != NULL) {
    PyBuffer_Release(buf);
  }
}

static bool weird_buffer(const Py_buffer *buf) {
  if (buf != NULL && buf->obj != NULL && buf->itemsize != 1) {
    PyErr_SetString(PyExc_ValueError, "buffer elements must be bytes");
    return true;
  }
  return false;
}

typedef struct {
  // clang-format gets confused by the (correct, documented) missing semicolon
  // after PyObject_HEAD.
  // clang-format off
  PyObject_HEAD
  blake3_hasher hasher;
  PyThread_type_lock lock;
  // NOTE: Any new fields here need to be handled in both _init() and _copy().
  // clang-format on
} Blake3Object;

static void Blake3_dealloc(Blake3Object *self) {
  PyThread_free_lock(self->lock);
  Py_TYPE(self)->tp_free((PyObject *)self);
}

static PyObject *Blake3_new(PyTypeObject *type, PyObject *args,
                            PyObject *kwds) {
  Blake3Object *self = NULL;
  PyThread_type_lock self_lock = NULL;
  Py_buffer data = {0};
  Py_buffer key = {0};
  const char *derive_key_context = NULL;
  Py_ssize_t max_threads = 1;
  int usedforsecurity = 1;

  PyObject *ret = NULL;

  static char *kwlist[] = {
      "", // data, positional-only
      "key",
      "derive_key_context",
      "max_threads",     // currently ignored
      "usedforsecurity", // currently ignored
      NULL,
  };
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

  self = (Blake3Object *)type->tp_alloc(type, 0);
  if (self == NULL) {
    goto exit;
  }

  // TODO: Hashlib implementations do an optimization where they avoid
  // allocating this lock unless it's needed. Is that worth it? It would mean
  // we'd need to handle the possible allocation failure at every lock site.
  // (Hashlib itself handles these by retaining the GIL rather than reporting
  // the error.)
  self_lock = PyThread_allocate_lock();
  if (self_lock == NULL) {
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
    if (data.len >= HASHLIB_GIL_MINSIZE) {
      // clang-format off
      Py_BEGIN_ALLOW_THREADS
      // This instance is not yet shared, so we don't need self->lock.
      blake3_hasher_update(&self->hasher, data.buf, data.len);
      Py_END_ALLOW_THREADS
      // clang-format on
    } else {
      // Don't bother releasing the GIL for short inputs.
      blake3_hasher_update(&self->hasher, data.buf, data.len);
    }
  }

  // success
  self->lock = self_lock;
  ret = (PyObject *)self;
  self_lock = NULL; // pass ownership
  self = NULL;      // pass ownership

exit:
  if (self != NULL) {
    Py_TYPE(self)->tp_free((PyObject *)self);
  }
  if (self_lock != NULL) {
    PyThread_free_lock(self_lock);
  }
  release_buf_if_acquired(&data);
  release_buf_if_acquired(&key);
  return ret;
}

// Used for long updates and long outputs.
static void Blake3_release_gil_and_lock_self(Blake3Object *self,
                                             PyThreadState **thread_state) {
  *thread_state = PyEval_SaveThread();
  PyThread_acquire_lock(self->lock, WAIT_LOCK);
}

static void Blake3_unlock_self_and_acquire_gil(Blake3Object *self,
                                               PyThreadState **thread_state) {
  PyThread_release_lock(self->lock);
  PyEval_RestoreThread(*thread_state);
}

// Used for shorter operations that touch self.
static void Blake3_lock_self(Blake3Object *self) {
// The optimistic locking strategy here is copied from CPython's ENTER_HASHLIB
// macro. If we port this code to hashlib, we should probably use that.
#ifdef ENTER_HASHLIB
#error Delete this helper function?
#endif
  if (!PyThread_acquire_lock(self->lock, NOWAIT_LOCK)) {
    // clang-format off
    Py_BEGIN_ALLOW_THREADS
    PyThread_acquire_lock(self->lock, WAIT_LOCK);
    Py_END_ALLOW_THREADS
    // clang-format on
  }
}

static void Blake3_unlock_self(Blake3Object *self) {
#ifdef LEAVE_HASHLIB
#error Delete this helper function too?
#endif
  PyThread_release_lock(self->lock);
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

  if (data.len >= HASHLIB_GIL_MINSIZE) {
    PyThreadState *thread_state;
    Blake3_release_gil_and_lock_self(self, &thread_state);
    blake3_hasher_update(&self->hasher, data.buf, data.len);
    Blake3_unlock_self_and_acquire_gil(self, &thread_state);
  } else {
    // Don't bother releasing the GIL for short inputs.
    Blake3_lock_self(self);
    blake3_hasher_update(&self->hasher, data.buf, data.len);
    Blake3_unlock_self(self);
  }

  // Success. We need to increment the refcount on self to return it, see:
  // https://docs.python.org/3/extending/extending.html#ownership-rules.
  Py_INCREF(self);
  ret = (PyObject *)self;

exit:
  release_buf_if_acquired(&data);
  return ret;
}

// This implementation doesn't actually use mmap; it just falls back to regular
// file reading. This mainly exists for compatibility with the Rust
// implementation's Python test suite.
// TODO: actually mmap
static PyObject *Blake3_update_mmap(Blake3Object *self, PyObject *args,
                                    PyObject *kwds) {
  PyBytesObject *path_bytes = NULL;
  FILE *file = NULL;
  PyObject *ret = NULL;

  static char *kwlist[] = {
      "path",
      NULL,
  };
  if (!PyArg_ParseTupleAndKeywords(args, kwds, "O&", kwlist,
                                   PyUnicode_FSConverter, &path_bytes)) {
    return NULL;
  }

  PyThreadState *thread_state;
  Blake3_release_gil_and_lock_self(self, &thread_state);

  file = fopen(PyBytes_AS_STRING(path_bytes), "r");
  if (!file) {
    goto exit;
  }

  char *buf[BUFSIZE];
  while (1) {
    size_t n = fread(buf, sizeof(char), BUFSIZE, file);
    if (ferror(file)) {
      goto exit;
    }
    blake3_hasher_update(&self->hasher, buf, n);
    if (feof(file)) {
      break;
    }
  }

  int fclose_ret = fclose(file);
  file = NULL;
  if (fclose_ret != 0) {
    goto exit;
  }

  // Success. We need to increment the refcount on self to return it, see:
  // https://docs.python.org/3/extending/extending.html#ownership-rules.
  Py_INCREF(self);
  ret = (PyObject *)self;

exit:
  if (file) {
    fclose(file);
  }
  Blake3_unlock_self_and_acquire_gil(self, &thread_state);
  if (!ret) {
    // XXX: The caller must hold the GIL to call PyErr_SetFromErrno, although
    // this is not documented.
    PyErr_SetFromErrno(PyExc_OSError);
  }
  Py_XDECREF(path_bytes);
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

  if (length >= HASHLIB_GIL_MINSIZE) {
    PyThreadState *thread_state;
    Blake3_release_gil_and_lock_self(self, &thread_state);
    blake3_hasher_finalize_seek(&self->hasher, seek,
                                (uint8_t *)PyBytes_AsString(output), length);
    Blake3_unlock_self_and_acquire_gil(self, &thread_state);
  } else {
    // Don't bother releasing the GIL for short outputs.
    Blake3_lock_self(self);
    blake3_hasher_finalize_seek(&self->hasher, seek,
                                (uint8_t *)PyBytes_AsString(output), length);
    Blake3_unlock_self(self);
  }

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
  Blake3_lock_self(self);
  blake3_hasher_reset(&self->hasher);
  Blake3_unlock_self(self);
  Py_RETURN_NONE;
}

static PyMethodDef Blake3_methods[] = {
    {"update", (PyCFunction)Blake3_update, METH_VARARGS, "add input bytes"},
    {"update_mmap", (PyCFunction)Blake3_update_mmap,
     METH_VARARGS | METH_KEYWORDS, "add input bytes from a filepath"},
    {"digest", (PyCFunction)Blake3_digest,
     METH_VARARGS | METH_KEYWORDS, "finalize the hash"},
    {"hexdigest", (PyCFunction)Blake3_hexdigest,
     METH_VARARGS | METH_KEYWORDS,
     "finalize the hash and encode the result as hex"},
    {"copy", (PyCFunction)Blake3_copy, METH_VARARGS,
     "make a copy of this hasher"},
    {"reset", (PyCFunction)Blake3_reset, METH_VARARGS,
     "reset this hasher to its initial state"},
    {NULL, NULL, 0, NULL} // sentinel
};

static PyTypeObject Blake3Type = {
    // clang-format gets confused by the (correct, documented) missing
    // semicolon after PyObject_HEAD_INIT.
    // clang-format off
    PyVarObject_HEAD_INIT(NULL, 0)
    .tp_name = "blake3.blake3",
    .tp_doc = "an incremental BLAKE3 hasher",
    .tp_basicsize = sizeof(Blake3Object),
    .tp_itemsize = 0,
    .tp_flags = Py_TPFLAGS_DEFAULT,
    .tp_new = Blake3_new,
    .tp_dealloc = (destructor) Blake3_dealloc,
    .tp_methods = Blake3_methods,
    // clang-format on
};

// Declared above but implemented here, because it needs to refer to Blake3Type.
static PyObject *Blake3_copy(Blake3Object *self, PyObject *args) {
  Blake3Object *copy = NULL;
  PyThread_type_lock copy_lock = NULL;

  PyObject *ret = NULL;

  copy = PyObject_New(Blake3Object, &Blake3Type);
  if (copy == NULL) {
    goto exit;
  }

  copy_lock = PyThread_allocate_lock();
  if (copy_lock == NULL) {
    goto exit;
  }

  Blake3_lock_self(self);
  memcpy(&copy->hasher, &self->hasher, sizeof(blake3_hasher));
  copy->lock = copy_lock;
  Blake3_unlock_self(self);

  // success
  ret = (PyObject *)copy;
  copy = NULL;      // pass the refcount
  copy_lock = NULL; // pass ownership

exit:
  Py_XDECREF(copy);
  if (copy_lock != NULL) {
    PyThread_free_lock(copy_lock);
  }
  return ret;
}

static struct PyModuleDef blake3module = {
    PyModuleDef_HEAD_INIT,
    .m_name = "blake3",
    .m_doc = SETUP_PY_DESCRIPTION,
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

  if (PyModule_AddObject(module, "blake3", (PyObject *)&Blake3Type) < 0) {
    Py_DECREF((PyObject *)&Blake3Type);
    goto exit;
  }

  if (PyModule_AddStringConstant(module, "__version__", SETUP_PY_VERSION) < 0) {
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
