#! /usr/bin/env python3

import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).parent.parent.parent

subprocess.run(["maturin", "sdist"], check=True)

sdists = [x for x in (ROOT / "target" / "wheels").iterdir()]
if len(sdists) != 1:
    raise RuntimeError("expected one sdist, found " + repr(sdists))

with open(os.environ["GITHUB_OUTPUT"], "a") as output:
    output.write(f"sdist_path={str(sdists[0])}\n")
