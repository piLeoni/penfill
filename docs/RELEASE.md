# Releasing penfill 0.2.0

Dry-runs pass locally: `cargo publish --dry-run`, `maturin build --release`,
`npm pack` in `bindings/node`.

## One-time setup

1. **GitHub** — create `pileoni/penfill`, push `master`, enable Actions.
2. **crates.io** — log in, add `CARGO_REGISTRY_TOKEN` as a GitHub Actions secret
   (or publish manually: `cargo publish`).
3. **PyPI** — register the project, configure [trusted publishing](https://docs.pypi.org/trusted-publishers/)
   for the release workflow (OIDC), or upload wheels from `maturin build`.
4. **npm** — create the `penfill` package, add `NPM_TOKEN` as a secret for
   provenance publish.

## Tag release

```sh
git tag v0.2.0
git push origin master --tags
```

On `v*` tags the workflow publishes **`penfill` to crates.io** (needs
`CARGO_REGISTRY_TOKEN`), uploads WASM and other build artifacts to the GitHub
release, and builds Python/Node packages without uploading them to PyPI/npm yet.

First crates.io upload (once, locally):

```sh
cargo login   # paste an API token from https://crates.io/settings/tokens
cargo publish --package penfill --locked
```

Or push tag `v0.2.0` after adding `CARGO_REGISTRY_TOKEN` to GitHub secrets.

## Version bump (later)

Keep in sync: root `Cargo.toml`, `[workspace.package].version`,
`bindings/python/pyproject.toml` (dynamic from Cargo), `bindings/node/package.json`,
then tag `vX.Y.Z`.
