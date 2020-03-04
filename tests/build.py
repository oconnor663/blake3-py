#! /usr/bin/env python3

# Build the shared library, and then copy it into the root of the project,
# using a destination filename that Python will be able to import on the curent
# platform. (That is, blake3.so on Linux and macOS, and blake3.pyd on Windows.)

from pathlib import Path
import shutil
import subprocess

HERE = Path(__file__).parent
ROOT = HERE / ".."

subprocess.run(["cargo", "build", "--release"], check=True, cwd=str(ROOT))

SRC_DEST = [
    ["libblake3.so", "blake3.so"],
    ["libblake3.dylib", "blake3.so"],
    ["blake3.dll", "blake3.pyd"],
]

for (src, dest) in SRC_DEST:
    source_path = ROOT / "target" / "release" / src
    destination_path = HERE / dest
    if source_path.exists():
        print("copying", source_path, "to", destination_path)
        shutil.copy2(str(source_path), str(destination_path))
