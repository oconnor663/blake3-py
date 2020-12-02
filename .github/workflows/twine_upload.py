#! /usr/bin/env python3

import github
import os
import shutil
import subprocess
import tempfile
import urllib

g = github.Github()  # read-only, no token needed
tag_name = os.environ["GITHUB_TAG"]
tag_prefix = "refs/tags/"
if tag_name.startswith(tag_prefix):
    tag_name = tag_name[len(tag_prefix):]
rerelease_suffix = "_rerelease"
if tag_name.endswith(rerelease_suffix):
    tag_name = tag_name[:len(tag_name) - len(rerelease_suffix)]
    print("This is a rerelease of {}.".format(tag_name))

repo = g.get_repo("oconnor663/blake3-py")

releases = list(repo.get_releases())
for release in releases:
    if release.tag_name == tag_name:
        break
else:
    raise RuntimeError("no release for tag " + repr(tag_name))

asset_names = [asset.name for asset in release.get_assets()]
asset_files = []

tempdir = tempfile.mkdtemp()
for asset_name in asset_names:
    urlbase = "https://github.com/oconnor663/blake3-py/releases/download/{}/{}"
    url = urlbase.format(tag_name, asset_name)
    filepath = os.path.join(tempdir, asset_name)
    asset_files.append(filepath)
    with urllib.request.urlopen(url) as request, open(filepath, "wb") as f:
        print("Downloading " + asset_name)
        shutil.copyfileobj(request, f)

print("Uploading to PyPI with twine...")
# Credentials are in the environment, TWINE_USERNAME and TWINE_PASSWORD.
twine_cmd = ["twine", "upload", "--skip-existing"] + asset_files
subprocess.run(twine_cmd, check=True)

print("Success!")
