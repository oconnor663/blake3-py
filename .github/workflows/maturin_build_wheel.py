#! /usr/bin/env python3

import sys
from pathlib import Path
import subprocess

ROOT = Path(__file__).parent.parent.parent

# There are generally several Python versions installed. Just package a wheel
# for the run that this CI is explicitly testing. Also, don't build the sdist;
# we'll do that in a separate job.
subprocess.run(
    ["maturin", "build", "--release", "--no-sdist", "--interpreter", sys.executable])

wheels = [x for x in (ROOT / "target" / "wheels").iterdir()]
if len(wheels) != 1:
    raise RuntimeError("expected one wheel, found " + repr(wheels))

print("::set-output name=wheel_path::" + str(wheels[0]))
print("::set-output name=wheel_name::" + wheels[0].name)
