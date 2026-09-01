# Contributing to Paperoll

Thanks for helping improve Paperoll.

## Development setup

Install the Rust toolchain through `rustup`. Paperoll uses the pinned toolchain
in `rust-toolchain.toml`. GPUI also needs the native platform build tools:

- macOS: Xcode Command Line Tools.
- Windows: Visual Studio C++ Build Tools, Windows SDK, and CMake.
- Ubuntu Linux: Clang, CMake, Vulkan, Wayland, X11, fontconfig, and related
  development libraries listed in `.github/workflows/ci.yml`.

Build and validate changes with:

```sh
./scripts/cargo.sh test --locked
./scripts/cargo.sh fmt --all -- --check
./scripts/cargo.sh clippy --all-targets --locked -- -D warnings
./scripts/build.sh --debug
```

On Windows, run the equivalent `cargo` commands directly from a Visual Studio
developer shell and use `bash scripts/build.sh windows` to package the binary.

## Pull requests

1. Keep changes focused and explain the user-visible outcome.
2. Add regression tests for behavior changes.
3. Run the checks above on your platform.
4. Do not commit generated `target/` or `dist/` content, credentials, signing
   material, or personal Paperoll data.

By contributing, you agree that your contribution is licensed under the
GNU General Public License v3.0 or later.
