#! /usr/bin/env python3

from pathlib import Path
import subprocess

ROOT = Path(__file__).parent.parent.parent

subprocess.run(["maturin", "sdist"], check=True)

sdists = [x for x in (ROOT / "target" / "wheels").iterdir()]
if len(sdists) != 1:
    raise RuntimeError("expected one sdist, found " + repr(sdists))

print("::set-output name=sdist_path::" + str(sdists[0]))
print("::set-output name=sdist_name::" + sdists[0].name)
