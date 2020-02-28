#! /usr/bin/env python3

# Build the shared library, and then copy it into the root of the project,
# using a destination filename that Python will be able to import on the curent
# platform. (That is, blake3.so on Linux and macOS, and blake3.pyd on Windows.)

from os import path
import shutil
import subprocess
import sys

HERE = path.dirname(__file__) or "."

subprocess.run(["cargo", "build", "--release"], check=True, cwd=HERE)

if sys.platform.startswith("win32") or sys.platform.startswith("cygwin"):
    source_prefix = ""
    source_extension = "dll"
    destination_extension = "pyd"
elif sys.platform.startswith("darwin"):
    source_prefix = "lib"
    source_extension = "dylib"
    destination_extension = "so"
else:
    if not sys.platform.startswith("linux"):
        print("Unknown platform, assuming Linux-like *.so filenames.")
    source_prefix = "lib"
    source_extension = "so"
    destination_extension = "so"

source_filename = source_prefix + "blake3." + source_extension
source = path.join(HERE, "target", "release", source_filename)
destination = path.join(HERE, "blake3." + destination_extension)
shutil.copy2(source, destination)
