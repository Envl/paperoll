#!/usr/bin/env bash
set -euo pipefail

profile=release
platform=host
target=

usage() {
    printf '%s\n' \
        "Usage: ./scripts/build.sh [macos|windows|linux] [--debug] [--target RUST_TARGET]" \
        "" \
        "Builds Paperoll for the current host by default." \
        "Cross-compilation requires the requested Rust target and a compatible linker."
}

while (($#)); do
    case "$1" in
        macos|windows|linux)
            platform=$1
            ;;
        --debug)
            profile=debug
            ;;
        --release)
            profile=release
            ;;
        --target)
            shift
            if (($# == 0)); then
                printf 'Missing value for --target\n' >&2
                exit 2
            fi
            target=$1
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf 'Unknown argument: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
    shift
done

script_directory=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
project_directory=$(cd -- "$script_directory/.." && pwd)
host_target=$(rustc -vV | sed -n 's/^host: //p')

case "$host_target" in
    *-apple-darwin) host_platform=macos ;;
    *-windows-*) host_platform=windows ;;
    *-linux-*) host_platform=linux ;;
    *)
        printf 'Unsupported host target: %s\n' "$host_target" >&2
        exit 1
        ;;
esac

if [[ $platform == host ]]; then
    platform=$host_platform
fi

if [[ -z $target ]]; then
    case "$platform" in
        macos)
            case "$host_target" in
                aarch64-*) target=aarch64-apple-darwin ;;
                *) target=x86_64-apple-darwin ;;
            esac
            ;;
        windows) target=x86_64-pc-windows-msvc ;;
        linux) target=x86_64-unknown-linux-gnu ;;
    esac
fi

case "$platform:$target" in
    macos:*-apple-darwin|windows:*-windows-*|linux:*-linux-*) ;;
    *)
        printf 'Target %s does not match platform %s\n' "$target" "$platform" >&2
        exit 2
        ;;
esac

if ! rustup target list --installed | grep -Fxq "$target"; then
    printf 'Rust target is not installed: %s\nRun: rustup target add %s\n' "$target" "$target" >&2
    exit 1
fi

if [[ $host_platform == macos && -z ${SDKROOT:-} ]]; then
    export SDKROOT
    SDKROOT=$(xcrun --sdk macosx --show-sdk-path)
fi

cargo_arguments=(build --target "$target")
if [[ $profile == release ]]; then
    cargo_arguments+=(--release)
fi

(cd -- "$project_directory" && cargo "${cargo_arguments[@]}")

target_directory="$project_directory/target/$target/$profile"
artifact_directory="$project_directory/dist/Paperoll-$platform-$target"
mkdir -p -- "$artifact_directory"

case "$platform" in
    macos)
        bundle_path="$artifact_directory/Paperoll.app"
        mkdir -p -- "$bundle_path/Contents/MacOS" "$bundle_path/Contents/Resources"
        install -m 755 "$target_directory/paperoll" "$bundle_path/Contents/MacOS/Paperoll"
        install -m 644 "$project_directory/resources/Info.plist" "$bundle_path/Contents/Info.plist"
        install -m 644 "$project_directory/resources/Paperoll.icns" "$bundle_path/Contents/Resources/Paperoll.icns"
        if [[ $host_platform == macos ]]; then
            codesign --force --sign - "$bundle_path"
        fi
        artifact=$bundle_path
        ;;
    windows)
        artifact="$artifact_directory/Paperoll.exe"
        install -m 755 "$target_directory/paperoll.exe" "$artifact"
        ;;
    linux)
        artifact="$artifact_directory/paperoll"
        install -m 755 "$target_directory/paperoll" "$artifact"
        ;;
esac

printf '%s\n' "$artifact"
