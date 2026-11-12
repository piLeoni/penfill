# Releasing penfill

CI builds every platform's packages; publishing is done from a terminal with
what it builds. No registry token lives on GitHub. The credentials are the
maintainer's: `~/.cargo/credentials.toml` (crates.io), `~/.pypirc` (PyPI,
an account-wide token — a project-scoped one cannot create a new project),
`~/.npmrc` (npm, a granular token with "bypass 2FA", or every publish asks
for a one-time password).

## 1. Version

One version everywhere: root `Cargo.toml` (`[package]` and
`[workspace.package]`), `bindings/node/package.json`; Python reads it from
Cargo. Commit, push `master`.

## 2. The crate

```sh
cargo test
cargo publish -p penfill
```

Dependents in sibling repos (draw.land) then move their `penfill` version.

## 3. Build every platform

```sh
gh workflow run release.yml -R piLeoni/penfill --ref master
gh run watch <run-id> -R piLeoni/penfill
gh run download <run-id> -R piLeoni/penfill -D /tmp/penfill-release
```

A cancelled macOS job (they time out now and then) is rerun with
`gh run rerun <run-id> --failed`. Artifact downloads count against the
account's Actions egress; past the limit GitHub answers 503 for a while.

## 4. PyPI

```sh
twine upload /tmp/penfill-release/wheel-*/*
```

Five wheels (manylinux x86_64 / aarch64, macOS arm64 / x86_64, Windows x64,
all abi3) and the sdist.

## 5. npm

From `bindings/node`, with the addons in `artifacts/`:

```sh
cp /tmp/penfill-release/addon-*/*.node artifacts/
npx napi create-npm-dirs
npx napi artifacts -d artifacts
npm publish --access public      # publishes npm/<platform>/ first, then penfill
rm -rf artifacts npm
```

npm's spam check refuses the name `penfill-win32-x64-msvc`, so the Windows
addon ships inside `penfill` itself: publish the four other platform
packages, then `penfill` with `penfill.win32-x64-msvc.node` added to `files`
and only those four in `optionalDependencies` (`--ignore-scripts`, so
`napi prepublish` does not try the fifth again). The generated `index.js`
loads a `penfill.<platform>.node` beside it before it looks for the
platform package, so Windows needs nothing else.

## 6. Check

```sh
pip install penfill==X.Y.Z && python -c "import penfill; print(len(penfill.FILLS))"
npm i penfill@X.Y.Z && node -e "console.log(require('penfill').fills().length)"
```
