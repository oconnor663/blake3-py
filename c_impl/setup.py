from collections import namedtuple
import os
from os import path
import platform
import setuptools
import subprocess
import sys

VERSION = "0.0.1"
DESCRIPTION = "experimental bindings for the BLAKE3 C implementation, API-compatible with the Rust-based blake3 module"

unix_asm_files = [
    "vendor/blake3_sse2_x86-64_unix.S",
    "vendor/blake3_sse41_x86-64_unix.S",
    "vendor/blake3_avx2_x86-64_unix.S",
    "vendor/blake3_avx512_x86-64_unix.S",
]

windows_msvc_asm_files = [
    "vendor/blake3_sse2_x86-64_windows_msvc.asm",
    "vendor/blake3_sse41_x86-64_windows_msvc.asm",
    "vendor/blake3_avx2_x86-64_windows_msvc.asm",
    "vendor/blake3_avx512_x86-64_windows_msvc.asm",
]

# TODO: Do we need these?
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


def targeting_x86_64():
    # We use *Python's* word size to determine whether we're targeting 64-bit,
    # not the machine's.
    assert sys.maxsize.bit_length() in (31, 63)
    return (
        platform.machine().lower() in ("x86_64", "amd64")
        and sys.maxsize.bit_length() == 63
    )


def targeting_x86_32():
    # We use *Python's* word size to determine whether we're targeting 64-bit,
    # not the machine's. Also I'm not exactly sure what the full set of
    # "machine" values is, and this is partly copying upstream build.rs.
    assert sys.maxsize.bit_length() in (31, 63)
    return (
        platform.machine().lower() in ("i386", "i586", "i686", "x86_64", "amd64")
        and sys.maxsize.bit_length() == 31
    )


def is_aarch64():
    return platform.machine().lower() == "aarch64"


def force_intrinsics():
    return os.environ.get("FORCE_INTRINSICS") == "1"


def compile_x86_intrinsics():
    object_files = []
    for filepath, unix_flags, win_flags in x86_intrinsics_files:
        cc = setuptools.distutils.ccompiler.new_compiler()
        if is_windows():
            args = ["/O2"] + win_flags
        else:
            args = ["-O3"] + unix_flags
        print(f"compiling {filepath} with {args}")
        object_files += cc.compile([filepath], extra_preargs=args)
    return object_files


def windows_ml64_path():
    vswhere_path = (
        r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
    )
    if not path.exists(vswhere_path):
        raise RuntimeError(vswhere_path + " doesn't exist.")
    vswhere_cmd = [
        vswhere_path,
        "-latest",
        "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "-products",
        "*",
        "-find",
        r"**\Hostx64\x64\ml64.exe",
    ]
    result = subprocess.run(vswhere_cmd, check=True, stdout=subprocess.PIPE, text=True)
    vswhere_output = result.stdout.strip()
    if not result.stdout:
        raise RuntimeError("vswhere.exe didn't output a path")
    ml64_path = vswhere_output.splitlines()[-1]
    if not path.exists(ml64_path):
        raise RuntimeError(ml64_path + " doesn't exist")
    return ml64_path


def compile_windows_msvc_asm():
    ml64 = windows_ml64_path()
    object_files = []
    for filepath in windows_msvc_asm_files:
        obj_path = path.splitext(filepath)[0] + ".obj"
        cmd = [ml64, "/Fo", obj_path, "/c", filepath]
        print(" ".join(cmd))
        subprocess.run(cmd, check=True)
        object_files.append(obj_path)
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
    if targeting_x86_64() and not force_intrinsics():
        if is_windows():
            print("including x86-64 MSVC assembly")
            # The cl.exe compiler on Windows doesn't support .asm files, so we
            # need to do all the shelling out to assemble these.
            # TODO: Do we ever want to use the Windows GNU assembly files?
            extra_objects = compile_windows_msvc_asm()
        else:
            print("including x86-64 Unix assembly")
            # On Unix we can give .S assembly files directly to the C compiler,
            # which is nice.
            extra_objects = unix_asm_files
    elif targeting_x86_32() or (targeting_x86_64() and force_intrinsics()):
        print("building x86 intrinsics")
        # The intrinsics files each need different compiler flags set.
        # Extension() doesn't support this, so we compile them explicitly.
        extra_objects = compile_x86_intrinsics()
    elif is_aarch64():
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
        define_macros=[
            ("SETUP_PY_VERSION", '"' + VERSION + '"'),
            ("SETUP_PY_DESCRIPTION", '"' + DESCRIPTION + '"'),
        ],
    )


if path.realpath(os.getcwd()) != path.realpath(path.dirname(__file__)):
    raise RuntimeError("running from another directory isn't supported")

setuptools.setup(
    name="blake3_experimental_c",
    version=VERSION,
    description=DESCRIPTION,
    long_description=open("README.md").read(),
    long_description_content_type="text/markdown",
    author="Jack O'Connor",
    author_email="oconnor663@gmail.com",
    license="CC0-1.0 OR Apache-2.0",
    url="https://github.com/oconnor663/blake3-py/tree/master/c_impl",
    ext_modules=[prepare_extension()],
)
