#!/bin/zsh

set -euo pipefail

script_directory=${0:A:h}

# A raw Mach-O executable does not provide macOS with the application-bundle
# metadata it needs for reliable activation. Keep the familiar development
# command, but launch the ad-hoc-signed debug app on macOS.
if [[ "$(uname -s)" == "Darwin" && "$#" -eq 1 && "$1" == "run" ]]; then
  exec "$script_directory/bundle.sh" --run
fi

# Xcode's standalone `cc` does not infer an SDK include path on this host.
# Resolve it at invocation time so Tree-sitter's C parsers can find libc while
# keeping the project portable across Xcode installations.
if [[ "$(uname -s)" == "Darwin" && -z "${SDKROOT:-}" ]]; then
  export SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
fi

exec cargo "$@"
