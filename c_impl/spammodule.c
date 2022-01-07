#define PY_SSIZE_T_CLEAN
#include <Python.h>

static PyObject *spam_system(PyObject *self, PyObject *args) {
  Py_buffer input;
  if (!PyArg_ParseTuple(args, "y*", &input)) {
    return NULL;
  }
  const char *prefix = "got: ";
  char array[100] = {0x42};
  if (input.len > (Py_ssize_t)(sizeof(array) - strlen(prefix))) {
    PyErr_SetString(PyExc_ValueError, "input too long");
    return NULL;
  }
  strcpy(array, prefix);
  memcpy(array + strlen(prefix), input.buf, input.len);
  PyObject *copy = Py_BuildValue("y#", array, strlen(prefix) + input.len);
  return copy;
}

static PyMethodDef SpamMethods[] = {
    {"system", spam_system, METH_VARARGS, "Execute a shell command."},
    {NULL, NULL, 0, NULL} /* Sentinel */
};

static struct PyModuleDef spammodule = {
    PyModuleDef_HEAD_INIT, // standard header
    "spam",                // name of module
    NULL,                  // module documentation, may be NULL
    -1, // size of per-interpreter state of the module, or -1 if the module
        // keeps state in global variables.
    SpamMethods,
};

PyMODINIT_FUNC PyInit_spam(void) { return PyModule_Create(&spammodule); }
