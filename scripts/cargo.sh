#!/bin/zsh

set -euo pipefail

# Xcode's standalone `cc` does not infer an SDK include path on this host.
# Resolve it at invocation time so Tree-sitter's C parsers can find libc while
# keeping the project portable across Xcode installations.
if [[ "$(uname -s)" == "Darwin" && -z "${SDKROOT:-}" ]]; then
  export SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
fi

exec cargo "$@"

