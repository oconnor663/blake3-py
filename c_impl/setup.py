from collections import namedtuple
import os
import platform
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

# path, unix_flags, win_flags
x86_intrinsics_files = [
    ("vendor/blake3_sse2.c", ["-msse2"], []),
    ("vendor/blake3_sse41.c", ["-msse4.1"], []),
    ("vendor/blake3_avx2.c", ["-mavx2"], ["/arch:AVX2"]),
    ("vendor/blake3_avx512.c", ["-mavx512f", "-mavx512vl"], ["/arch:AVX512"]),
]


def is_windows():
    return sys.platform.startswith("win32")


def force_intrinsics():
    return os.environ.get("FORCE_INTRINSICS") == "1"


def compile_x86_intrinsics():
    object_files = []
    for path, unix_flags, win_flags in x86_intrinsics_files:
        cc = setuptools.distutils.ccompiler.new_compiler()
        if is_windows():
            args = win_flags
        else:
            args = unix_flags
        print(f"compiling {path} with {args}")
        object_files += cc.compile([path], extra_preargs=args)
    return object_files


def prepare_extension():
    sources = [
        "blake3module.c",
        "vendor/blake3.c",
        "vendor/blake3_dispatch.c",
        "vendor/blake3_portable.c",
    ]
    target = platform.machine()
    extra_objects = []
    if target == "x86_64" and not force_intrinsics():
        # TODO: Do we ever want to use the Windows GNU assembly files?
        if is_windows():
            print("including x86-64 MSVC assembly")
            extra_objects = windows_msvc_asm_files
        else:
            print("including x86-64 Unix assembly")
            extra_objects = unix_asm_files
    elif target in ("i386", "i686") or (target == "x86_64" and force_intrinsics()):
        print("building x86 intrinsics")
        extra_objects = compile_x86_intrinsics()
    elif target == "aarch64":
        print("including NEON intrinsics")
        # Compiling NEON intrinsics doesn't require extra flags on AArch64.
        sources.append("vendor/blake3_neon.c")
    else:
        print("portable code only")

    return setuptools.Extension(
        "blake3",
        sources=sources,
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
