#!/bin/zsh
set -euo pipefail

profile=debug
cargo_args=()
should_run=false

for argument in "$@"; do
    case "$argument" in
        --release)
            profile=release
            cargo_args+=(--release)
            ;;
        --run)
            should_run=true
            ;;
        *)
            print -u2 "Unknown argument: $argument"
            exit 2
            ;;
    esac
done

script_directory=${0:A:h}
project_directory=${script_directory:h}
bundle_path="$project_directory/target/$profile/Paperoll.app"

"$script_directory/cargo.sh" build "${cargo_args[@]}"
install -d "$bundle_path/Contents/MacOS" "$bundle_path/Contents/Resources"
install -m 755 "$project_directory/target/$profile/paperoll" "$bundle_path/Contents/MacOS/Paperoll"
install -m 644 "$project_directory/resources/Info.plist" "$bundle_path/Contents/Info.plist"
install -m 644 "$project_directory/resources/Paperoll.icns" "$bundle_path/Contents/Resources/Paperoll.icns"
codesign --force --sign - "$bundle_path"

print "$bundle_path"
if $should_run; then
    open "$bundle_path"
fi
