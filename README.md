# Paperoll
<img width="2356" height="1916" alt="CleanShot 2026-09-02 at 14 26 40@2x" src="https://github.com/user-attachments/assets/5dca4b24-b9fa-4faa-8c92-50b20b74a65d" />

Paperoll is a native scratchpad built with Rust, GPUI, and
[GPUI Component](https://github.com/longbridge/gpui-component).

[![CI](https://github.com/Envl/paperoll/actions/workflows/ci.yml/badge.svg)](https://github.com/Envl/paperoll/actions/workflows/ci.yml)
[![License: GPL v3+](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

Paperoll's block-based scratchpad model is inspired by
[Heynote](https://github.com/heyman/heynote/):

- every tab is a persistent roll of numbered snippet pages;
- drag tabs directly to reorder rolls; closing a roll always asks for confirmation;
- press `Command-Enter` to insert a page below the focused page;
- snippets grow with their text; the roll scrolls instead of individual editors;
- adjacent snippets alternate neutral backgrounds in light and dark mode, with a
  left-edge marker identifying the active block;
- use the independent number rail to see the active page and jump to any snippet;
- clear a page, then press Backspace once more to discard that empty page;
- syntax is detected per page and highlighted with Tree-sitter, with a per-snippet
  language menu for explicit overrides;
- press `Option-Shift-F` to format the focused snippet with Paperoll's built-in
  formatter when its detected or selected language is supported;
- `Command-T` creates a roll and `Command-W` closes the active roll.

Highlighting includes plain text, Bash, C, C++, C#, CSS, Go, HTML, Java,
JavaScript, JSON, JSON Lines, Kotlin, Lua, Markdown, PHP, Python, Ruby, Rust, SQL, Swift,
TOML, TSX, TypeScript, XML, YAML, and Zig.

## Downloads

Tagged releases publish macOS disk images plus signed updater packages for macOS,
Windows, and Linux on the [GitHub Releases page](https://github.com/Envl/paperoll/releases).
The macOS apps and disk images are Developer ID signed and notarized by Apple.
Updater packages are separately signed with Paperoll's updater key. Windows
Authenticode signing is not yet configured, so Windows may require an explicit
first-launch approval.

Storage is deliberately just folders and text files. A roll/tab is a numbered
folder and each snippet is a numbered file inside it:

```text
rolls/
  001 Roll 1/
    001
    002.rs
  002 Notes/
    001.md
```

The number prefixes preserve tab and snippet order. An extensionless snippet
uses automatic language detection; choosing a language adds its conventional
file extension, which also restores that selection on the next launch. Paperoll
stores this `rolls` directory in the platform application-data directory and
never writes into the source checkout at runtime.

## Development

Paperoll uses the Rust version pinned in `rust-toolchain.toml`. Install Rust
with `rustup`, then install the native GPUI prerequisites for your platform:

- macOS: Xcode Command Line Tools.
- Windows: Visual Studio C++ Build Tools, Windows SDK, and CMake.
- Linux: Clang, CMake, Vulkan, Wayland, X11, and fontconfig development
  packages. CI contains the authoritative Ubuntu package list.

```sh
./scripts/cargo.sh fetch
./scripts/cargo.sh run
./scripts/cargo.sh test
./scripts/cargo.sh clippy --all-targets -- -D warnings
./scripts/cargo.sh build --release
```

Build a launchable, ad-hoc-signed macOS app bundle:

```sh
./scripts/bundle.sh --release
open target/release/Paperoll.app
```

Build a release artifact for the current platform, or select macOS, Windows,
or Linux explicitly:

```sh
./scripts/build.sh
./scripts/build.sh macos
./scripts/build.sh windows
./scripts/build.sh linux
```

Artifacts are written under `dist/Paperoll-<platform>-<target>/`. A selected
cross-platform target must be installed with `rustup target add` and needs a
compatible linker. Override the default target when needed, for example:

```sh
./scripts/build.sh linux --target aarch64-unknown-linux-gnu
```

Set `PAPEROLL_WORKSPACE_PATH` to a directory path to isolate roll and snippet
storage while developing or testing.

## Releases

The GitHub Actions release workflow validates that a `v*` tag matches the
version in `Cargo.toml`, builds signed update packages for macOS on Apple Silicon
and Intel, Windows, and Linux on native runners, generates `latest.json` and
checksums, and publishes a GitHub release:

```sh
git tag v0.2.0
git push origin v0.2.0
```

Protect release tags and the `main` branch in GitHub before publishing. Pushing
a release tag is the publication trigger; creating a local tag does not publish
anything.

### Auto-update signing

The updater checks the stable `latest.json` release asset after launch. Its
button appears beside the shortcut hints only when a newer signed version is
available. Clicking it downloads, verifies, and installs the platform package.

The generated private updater key is stored locally at `.release/update.key`
and is ignored by Git. Its password is in macOS Keychain under
`Paperoll Update Signing`. Back up both before the first public release. Once
the GitHub repository exists, configure the release environment secrets:

```sh
gh secret set --env release CARGO_PACKAGER_SIGN_PRIVATE_KEY < .release/update.key
security find-generic-password \
  -a "$USER" \
  -s "Paperoll Update Signing" \
  -w | gh secret set --env release CARGO_PACKAGER_SIGN_PRIVATE_KEY_PASSWORD
```

macOS releases additionally require these `release` environment secrets:

- `APPLE_CERTIFICATE_P12_BASE64`: Base64-encoded Developer ID Application
  certificate and private key exported as PKCS #12.
- `APPLE_CERTIFICATE_PASSWORD`: password protecting that PKCS #12 export.
- `APPLE_API_KEY_ID`, `APPLE_API_ISSUER_ID`, and
  `APPLE_API_PRIVATE_KEY_BASE64`: App Store Connect API credentials accepted by
  Apple's notarization service.

The release workflow imports the certificate into an ephemeral keychain, signs
the app with hardened runtime and a secure timestamp, waits for Apple
notarization, staples and validates the ticket, and only then creates the signed
updater archive. The temporary keychain is deleted even when the job fails.

Never rotate or discard this key casually: existing installations trust the
public half embedded in `resources/update.pub`.

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) for setup and
pull-request expectations. Report vulnerabilities through GitHub private
vulnerability reporting as described in [SECURITY.md](SECURITY.md).

## License

Paperoll is free software licensed under the GNU General Public License v3.0 or
later. See [LICENSE](LICENSE). Third-party dependencies remain under their own
licenses.
