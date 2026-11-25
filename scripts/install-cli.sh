#!/usr/bin/env bash
# Install to a user-owned directory. Never changes shell rc files or needs sudo.
set -euo pipefail
prefix="${MIY_INSTALL_DIR:-$HOME/.local/bin}"
source_binary=""
server=""
no_input=false
usage() {
  echo 'Usage: bash install-cli.sh [--from /path/to/miy] [--prefix ~/.local/bin] [--server URL] [--no-input]' >&2
}
while [ "$#" -gt 0 ]; do
  case "$1" in
    --prefix|--from|--server)
      if [ "$#" -lt 2 ] || [ -z "$2" ]; then usage; exit 2; fi
      case "$1" in
        --prefix) prefix="$2" ;;
        --from) source_binary="$2" ;;
        --server) server="$2" ;;
      esac
      shift 2 ;;
    --no-input) no_input=true; shift ;;
    *) usage; exit 2 ;;
  esac
done
installer_dir="$(cd "$(dirname "$0")" && pwd)"
if [ -z "$source_binary" ] && [ -f "$installer_dir/release.json" ] && [ -x "$installer_dir/miy" ]; then
  source_binary="$installer_dir/miy"
fi
if [ -z "$source_binary" ]; then
  root="$(cd "$(dirname "$0")/.." && pwd)"
  cargo="$(command -v cargo || true)"
  if [ -z "$cargo" ] && [ -x "$HOME/.cargo/bin/cargo" ]; then cargo="$HOME/.cargo/bin/cargo"; fi
  if [ -z "$cargo" ]; then echo 'Install Rust via https://rustup.rs first, or use --from with a prebuilt miy binary.' >&2; exit 1; fi
  (cd "$root" && "$cargo" build --release --locked --no-default-features -p miy)
  source_binary="$root/target/release/miy"
fi
[ -x "$source_binary" ] || { echo 'Source must be an executable miy binary for this OS/CPU.' >&2; exit 1; }
"$source_binary" --version
mkdir -p "$prefix"
temporary="$(mktemp "$prefix/.miy-install.XXXXXX")"
trap 'rm -f "$temporary"' EXIT
install -m 755 "$source_binary" "$temporary"
mv -f "$temporary" "$prefix/miy"
echo "Installed: $prefix/miy"
context_args=(--output json --no-input context init)
if [ -n "$server" ]; then context_args+=(--url "$server"); fi
if setup_result="$("$prefix/miy" "${context_args[@]}")"; then
  echo "$setup_result"
  if [[ "$setup_result" =~ \"(setupRequired|serverMismatch)\"[[:space:]]*:[[:space:]]*true ]]; then
    echo 'Use the context recovery instructions above, then run miy auth status.'
  elif status_result="$(cd "$prefix" && unset MIY_CONTEXT && ./miy --output json --no-input auth status 2>/dev/null)"; then
    echo 'Already signed in. For an existing app, run the global miy app setup in its project.'
  elif [[ "$status_result" =~ \"code\"[[:space:]]*:[[:space:]]*\"(login_required|session_expired|unauthorized)\" ]]; then
    echo 'Next: miy auth login'
  else
    echo 'Authentication status could not be verified. Inspect miy auth status before signing in.'
  fi
else
  status=$?
  # Only the typed missing-address result may defer setup. Unsupported CLIs,
  # invalid supplied URLs and damaged configurations must remain failures.
  missing_url='"code"[[:space:]]*:[[:space:]]*"server_url_required"'
  if [ -z "$server" ] && [ "$status" -eq 2 ] && [[ "$setup_result" =~ $missing_url ]]; then
    if [ "$no_input" = false ] && [ -t 0 ] && [ -t 1 ] && [ -t 2 ]; then
      if "$prefix/miy" --output text context init; then
        echo 'Server registered. Next: miy auth login'
      else
        status=$?
        if [ "$status" -ne 130 ]; then
          echo 'The CLI binary is installed, but server registration failed. Run miy context init to retry.' >&2
          exit "$status"
        fi
        echo '{"installed":true,"contextInitialization":{"status":"required","setupRequired":true},"next":["miy context init","miy auth login"]}'
      fi
    else
      echo '{"installed":true,"contextInitialization":{"status":"required","setupRequired":true},"next":["miy context init","miy auth login"]}'
      echo 'No target server is configured. Run miy context init to enter its URL before login.'
    fi
  else
    echo "$setup_result"
    echo 'The CLI binary is installed, but server configuration could not be initialized. Resolve the reported error and rerun the installer or miy context init.' >&2
    exit "$status"
  fi
fi
echo "If needed, add $prefix to PATH. To create an app: miy app init ./my-app --id my-app --name 'My app'"

echo "To update an existing project with the same SDK bundle, run inside it: miy app setup"
echo "This updates its CLI, skills and manual. For a different SDK bundle, use miy app upgrade."
