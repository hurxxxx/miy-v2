#!/usr/bin/env bash
# Install to a user-owned directory. Never changes shell rc files or needs sudo.
set -euo pipefail
prefix="${MIY_INSTALL_DIR:-$HOME/.local/bin}"
source_binary=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --prefix) prefix="$2"; shift 2 ;;
    --from) source_binary="$2"; shift 2 ;;
    *) echo 'Usage: bash scripts/install-cli.sh [--from /path/to/miy] [--prefix ~/.local/bin]' >&2; exit 2 ;;
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
echo "If needed, add $prefix to PATH. Then run: miy app init ./my-app --id my-app --name 'My app'"

echo "To update an existing project with the same SDK bundle, run inside it: miy app setup"
echo "This updates its CLI, skills and manual. For a different SDK bundle, use miy app upgrade."
