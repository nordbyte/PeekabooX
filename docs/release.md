# Release Process

PeekabooX releases use one version across the Rust workspace, Python package,
and Python import metadata:

- `Cargo.toml` `workspace.package.version`
- `python/pyproject.toml` `project.version`
- `python/src/peekaboox/__init__.py` `__version__`
- `python/src/peekaboox/mcp/server.py` `SERVER_VERSION`

The release tag format is `vVERSION`, for example `v1.0.0`. Each release also
requires a `CHANGELOG.md` entry using `## VERSION - YYYY-MM-DD`.

## Local Validation

```bash
python3 packaging/release_manifest.py --check
python3 -m pip wheel --no-deps -w target/python-wheel ./python
cargo build --release -p peekaboox-cli -p peekabooxd
python3 packaging/debian/build_deb.py --skip-cargo-build
python3 packaging/smoke_install.py --skip-cargo-install
python3 packaging/release_manifest.py
```

`packaging/release_manifest.py` writes:

- `target/dist/release-manifest.json`
- `target/dist/SHA256SUMS`

If `target/dist/docker-image.json` exists, it is included as a release artifact
and embedded into the manifest.

## CI Release

The `Release` workflow runs on `v*` tags and manual dispatch. It builds the
Python wheel, Rust binaries, Debian package, Docker smoke image metadata, release
manifest, and SHA256 checksums, then uploads them as GitHub Actions artifacts.
On tag builds, it also creates the GitHub Release and attaches the wheel, Debian
package, Docker metadata, manifest, and checksum files as release assets.
