from collections import namedtuple
import os
import setuptools
import sys

HERE = os.path.dirname(__file__)
os.chdir(HERE)

unix_asm_files = [
    "vendor/blake3_sse2_x86-64_unix.S",
    "vendor/blake3_sse41_x86-64_unix.S",
    "vendor/blake3_avx2_x86-64_unix.S",
    "vendor/blake3_avx512_x86-64_unix.S",
]

windows_msvc_asm_files = [
    "vendor/blake3_sse2_x86-64_windows_msvc.S",
    "vendor/blake3_sse41_x86-64_windows_msvc.S",
    "vendor/blake3_avx2_x86-64_windows_msvc.S",
    "vendor/blake3_avx512_x86-64_windows_msvc.S",
]

windows_gnu_asm_files = [
    "vendor/blake3_sse2_x86-64_windows_gnu.S",
    "vendor/blake3_sse41_x86-64_windows_gnu.S",
    "vendor/blake3_avx2_x86-64_windows_gnu.S",
    "vendor/blake3_avx512_x86-64_windows_gnu.S",
]

IntrinsicsFile = namedtuple("IntrinsicsFile", ["path", "unix_flags", "win_flags"])
intrinsics_files = [
    IntrinsicsFile("vendor/blake3_sse2.c", ["-msse2"], []),
    IntrinsicsFile("vendor/blake3_sse41.c", ["-msse4.1"], []),
    IntrinsicsFile("vendor/blake3_avx2.c", ["-mavx2"], ["/arch:AVX2"]),
    IntrinsicsFile(
        "vendor/blake3_avx512.c", ["-mavx512f", "-mavx512vl"], ["/arch:AVX512"]
    ),
]


def use_intrinsics():
    return os.environ.get("USE_INTRINSICS") == "1"


def is_windows():
    return sys.platform.startswith("win32")


def compile_intrinsics():
    object_files = []
    for path, unix_flags, win_flags in intrinsics_files:
        cc = setuptools.distutils.ccompiler.new_compiler()
        if is_windows():
            args = win_flags
        else:
            args = unix_flags
        print(f"compiling {path} with {args}")
        object_files += cc.compile([path], extra_preargs=args)
    return object_files


def prepare_extension():
    if use_intrinsics():
        extra_objects = compile_intrinsics()
    elif is_windows():
        extra_objects = windows_msvc_asm_files
    else:
        # TODO: Do we ever want to use the Windows GNU assembly files?
        extra_objects = unix_asm_files

    return setuptools.Extension(
        "blake3",
        sources=[
            "blake3module.c",
            "vendor/blake3.c",
            "vendor/blake3_dispatch.c",
            "vendor/blake3_portable.c",
        ],
        include_dirs=[
            "vendor",
        ],
        extra_objects=extra_objects,
    )


setuptools.setup(
    name="blake3",
    version="0.0.0",
    description="experimental bindings for the BLAKE3 C implementation",
    ext_modules=[prepare_extension()],
)
