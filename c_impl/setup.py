from setuptools import setup, Extension

extension = Extension(
    "blake3",
    sources=[
        "blake3module.c",
        "vendor/blake3.c",
        "vendor/blake3_dispatch.c",
        "vendor/blake3_portable.c",
    ],
    extra_objects=[
        "vendor/blake3_sse2_x86-64_unix.S",
        "vendor/blake3_sse41_x86-64_unix.S",
        "vendor/blake3_avx2_x86-64_unix.S",
        "vendor/blake3_avx512_x86-64_unix.S",
    ],
    include_dirs=[
        "vendor",
    ],
)

setup(
    name="blake3",
    version="0.0.0",
    description="experimental bindings for the BLAKE3 C implementation",
    ext_modules=[extension],
)
