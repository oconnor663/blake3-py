### Releasing a new version

This project has GitHub Actions set up to automatically compile binary wheels
and upload build artifacts to GitHub and PyPI. This is triggered when a new tag
is pushed. To do a release:

1. Bump the version number in `Cargo.toml`.
2. Make a release commit.
3. Push `master` and make sure GitHub CI is green.
4. Tag the release commit and push the tag.

The rest is automatic. For more details, see `tag.yml` and the scripts that it
calls.

### When a new Python version comes out

When a new Python version comes out, it needs to be added to our CI configs in
several places. See commit [`e54c5a9`](https://github.com/oconnor663/blake3-py/commit/e54c5a94eecca6b9decf7c11588ab9971402276c)
(which added support for Python 3.10) as an example. The comments in
`maturin_build_wheel.py`, `push.yml`, and `tag.yml` should all refer to each
other, to help us avoid forgetting a spot.

To retroactively add new wheels to an existing release, tag the config change
commit with the original release version plus the suffix `_rerelease`. For
example, to build new wheels for version 0.2.1, push a tag called
`0.2.1_rerelease`. The upload step of the rerelease job will automatically skip
assets that already exist. For more details, search for `_rerelease` in
`upload_github_release_asset.py` and `twine_upload.py`.
