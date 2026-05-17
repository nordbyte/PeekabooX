# Packaging

This directory contains the supported local packaging paths for PeekabooX.

## Rust Binaries

Install the CLI and daemon from the workspace:

```bash
packaging/install-rust.sh
```

Set `PEEKABOOX_INSTALL_FEATURES=pipewire-backend,egl-backend` to install feature
builds that link against native PipeWire/EGL/GLES development libraries.

Equivalent direct commands:

```bash
cargo install --locked --path cli --bin peekaboox
cargo install --locked --path rust/daemon --bin peekabooxd
```

## Python Package

Build a wheel from the Python package directory:

```bash
python3 -m pip wheel --no-deps -w target/python-wheel ./python
```

Install it locally:

```bash
python3 -m pip install ./target/python-wheel/peekaboox-*.whl
```

## Debian Package

Validate package metadata without building:

```bash
python3 packaging/debian/build_deb.py --check
```

Build a local `.deb`:

```bash
python3 packaging/debian/build_deb.py
```

The package contains `peekaboox`, `peekabooxd`, systemd user units, docs, and
example plugins. Optional GPU capture builds can be produced with:

```bash
PEEKABOOX_DEB_FEATURES=pipewire-backend,egl-backend python3 packaging/debian/build_deb.py
```

## Install Smoke Tests

Run smoke checks against built artifacts:

```bash
python3 packaging/smoke_install.py --skip-cargo-install
python3 packaging/smoke_install.py --skip-wheel --skip-deb
```

The first command verifies the latest wheel and `.deb` artifacts. The second
command performs a real debug-mode `cargo install` of `peekaboox` and
`peekabooxd` into `target/install-smoke/cargo`.

## Release Manifest

Validate release version consistency and changelog coverage:

```bash
python3 packaging/release_manifest.py --check
```

After building the wheel and `.deb`, write the release manifest and checksums:

```bash
python3 packaging/release_manifest.py
```

The script validates `Cargo.toml`, `python/pyproject.toml`,
`python/src/peekaboox/__init__.py`, `python/src/peekaboox/mcp/server.py`,
`flake.nix`, and `CHANGELOG.md`, then writes `target/dist/release-manifest.json` and
`target/dist/SHA256SUMS`. If
`target/dist/docker-image.json` exists, it is included as Docker image metadata.
The full release workflow is documented in `docs/release.md`.

## Docker

Build a runtime image:

```bash
docker build -t peekaboox:local .
```

Run the Docker smoke target:

```bash
docker build --target smoke -t peekaboox:smoke .
```

Desktop capture and input automation still require host desktop services and
device/socket access at runtime; the image is primarily a packaged CLI/daemon
runtime and CI install-smoke environment.

## Nix

Enter the development shell:

```bash
nix develop
```

Build the default package:

```bash
nix build
```

The flake packages the Rust CLI/daemon and installs docs, systemd user units,
workflow examples, and plugin examples.
