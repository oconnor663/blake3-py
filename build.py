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

SRC_DEST = [
    ["libblake3.so", "blake3.so"],
    ["libblake3.dylib", "blake3.so"],
    ["blake3.dll", "blake3.pyd"],
]

for (src, dest) in SRC_DEST:
    source_path = path.join(HERE, "target", "release", src)
    destination_path = path.join(HERE, dest)
    if path.exists(source_path):
        print("copying", source_path, "to", destination_path)
        shutil.copy2(source_path, destination_path)
