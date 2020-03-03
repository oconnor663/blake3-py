#! /usr/bin/env python3

import github
import os
import sys

RETRIES = 10

g = github.Github(os.environ["GITHUB_TOKEN"])
tag_name = os.environ["GITHUB_TAG"]
tag_prefix = "refs/tags/"
if tag_name.startswith(tag_prefix):
    tag_name = tag_name[len(tag_prefix):]
assert len(sys.argv) == 2
asset_path = sys.argv[1]

repo = g.get_repo("oconnor663/blake3-py")

tags = list(repo.get_tags())

for tag in tags:
    if tag.name == tag_name:
        break
else:
    raise RuntimeError("no tag named " + repr(tag_name))

try:
    print("Creating GitHub release for tag " + repr(tag_name) + "...")
    repo.create_git_release(tag_name, tag_name, tag.commit.commit.message)
except github.GithubException as e:
    if e.data["errors"][0]["code"] == "already_exists":
        print("Release for tag " + repr(tag_name) + " already exists.")
    else:
        raise

releases = list(repo.get_releases())
for release in releases:
    if release.tag_name == tag_name:
        break
else:
    raise RuntimeError("no release for tag " + repr(tag_name))

print("Uploading " + repr(asset_path) + "...")
for i in range(RETRIES):
    try:
        release.upload_asset(asset_path)
        break
    except Exception as e:
        print("Upload attempt #{} failed:".format(i))
        print(e)
else:
    raise RuntimeError("All upload attempts failed.")

print("Success!")
