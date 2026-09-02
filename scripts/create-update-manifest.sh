#!/usr/bin/env bash
set -euo pipefail

if (($# != 4)); then
    printf 'Usage: %s VERSION REPOSITORY ASSETS_DIRECTORY OUTPUT\n' "$0" >&2
    exit 2
fi

version=$1
repository=$2
assets_directory=$3
output=$4
tag="v$version"
base_url="https://github.com/$repository/releases/download/$tag"

asset() {
    local filename=$1
    local format=$2
    local signature_file="$assets_directory/$filename.sig"
    [[ -f "$assets_directory/$filename" ]] || {
        printf 'Missing update artifact: %s\n' "$filename" >&2
        exit 1
    }
    [[ -f $signature_file ]] || {
        printf 'Missing update signature: %s.sig\n' "$filename" >&2
        exit 1
    }
    jq -n \
        --arg url "$base_url/$filename" \
        --rawfile signature "$signature_file" \
        --arg format "$format" \
        '{url: $url, signature: $signature, format: $format}'
}

macos_aarch64=$(asset "Paperoll-$version-macos-aarch64.app.tar.gz" app)
macos_x86_64=$(asset "Paperoll-$version-macos-x86_64.app.tar.gz" app)
windows_x86_64=$(asset "Paperoll-$version-windows-x86_64-setup.exe" nsis)
linux_x86_64=$(asset "Paperoll-$version-linux-x86_64.AppImage" appimage)

jq -n \
    --arg version "$version" \
    --argjson macos_aarch64 "$macos_aarch64" \
    --argjson macos_x86_64 "$macos_x86_64" \
    --argjson windows_x86_64 "$windows_x86_64" \
    --argjson linux_x86_64 "$linux_x86_64" \
    '{
        version: $version,
        platforms: {
            "macos-aarch64": $macos_aarch64,
            "macos-x86_64": $macos_x86_64,
            "windows-x86_64": $windows_x86_64,
            "linux-x86_64": $linux_x86_64
        }
    }' > "$output"
