#!/usr/bin/env bash
# Served by Core with a configured origin and immutable, verified asset hashes.
# Keep all work inside the function: an interrupted download must not run a
# partially received installer. Requires Bash, curl, tar and SHA-256 utilities.
miy_install_online() (
  set -euo pipefail
  server=@MIY_SERVER@
  version=@MIY_VERSION@
  prefix="${MIY_INSTALL_DIR:-$HOME/.local/bin}"
  modify_path=true
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --prefix) [ "$#" -ge 2 ] && [ -n "$2" ] || { echo 'Missing --prefix directory' >&2; exit 2; }; prefix="$2"; shift 2 ;;
      --no-modify-path) modify_path=false; shift ;;
      *) echo 'Usage: bash [--prefix DIRECTORY] [--no-modify-path]' >&2; exit 2 ;;
    esac
  done
  case "$prefix" in /*) ;; *) echo 'Install directory must be an absolute path.' >&2; exit 2 ;; esac
  case "$prefix" in *$'\n'*|*$'\r'*) echo 'Invalid install directory.' >&2; exit 2 ;; esac
  for tool in curl tar mktemp; do command -v "$tool" >/dev/null || { echo "Required tool: $tool" >&2; exit 1; }; done
  case "$(uname -s):$(uname -m)" in
# @MIY_ASSETS@
    *) echo 'Supported systems: Linux and macOS, x64 or ARM64.' >&2; exit 1 ;;
  esac
  if command -v sha256sum >/dev/null; then
    hash_file() { sha256sum "$1" | cut -d ' ' -f 1; }
  elif command -v shasum >/dev/null; then
    hash_file() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
  else
    echo 'Install sha256sum or shasum before running this installer.' >&2; exit 1
  fi
  temporary=$(mktemp -d "${TMPDIR:-/tmp}/miy-install.XXXXXXXX")
  trap 'rm -rf "$temporary"' EXIT
  protocols='=https'
  case "$server" in http://*) protocols='=http,https' ;; esac
  echo "Downloading miy $version for $(uname -s)/$(uname -m)..."
  curl --fail --silent --show-error --location --proto "$protocols" --proto-redir "$protocols" \
    --connect-timeout 20 --max-time 600 --retry 2 --output "$temporary/cli.tar.gz" "$url"
  [ "$(hash_file "$temporary/cli.tar.gz")" = "$sha256" ] || { echo 'CLI archive checksum mismatch; nothing installed.' >&2; exit 1; }
  entries=$(tar -tzf "$temporary/cli.tar.gz" | LC_ALL=C sort)
  [ "$entries" = $'install-cli.sh\nmiy\nrelease.json' ] || { echo 'Invalid CLI archive contents.' >&2; exit 1; }
  tar -xzf "$temporary/cli.tar.gz" -C "$temporary" miy install-cli.sh release.json
  [ -f "$temporary/miy" ] && [ ! -L "$temporary/miy" ] && [ -f "$temporary/install-cli.sh" ] && [ ! -L "$temporary/install-cli.sh" ]
  [ "$(hash_file "$temporary/miy")" = "$binary_sha256" ] || { echo 'CLI binary checksum mismatch; nothing installed.' >&2; exit 1; }
  [ "$("$temporary/miy" --version)" = "miy $version" ] || { echo 'CLI version mismatch or incompatible system.' >&2; exit 1; }
  bash "$temporary/install-cli.sh" --from "$temporary/miy" --prefix "$prefix" --server "$server" --no-input
  if [ "$modify_path" = true ]; then
    # Append only to this user's shell files, preserving their existing content.
    printf -v path_line 'export PATH=%q:"$PATH"' "$prefix"
    profiles=("$HOME/.profile")
    case "${SHELL:-/bin/bash}" in
      */zsh) profiles+=("${ZDOTDIR:-$HOME}/.zshrc") ;;
      */bash) profiles+=("$HOME/.bashrc");
        if [ -f "$HOME/.bash_profile" ]; then profiles+=("$HOME/.bash_profile");
        elif [ -f "$HOME/.bash_login" ]; then profiles+=("$HOME/.bash_login"); fi ;;
      */fish)
        mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d"
        # Fish supports the backslash escapes produced by printf %q for paths.
        printf -v fish_line 'fish_add_path -- %q' "$prefix"
        fish_profile="${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d/miy.fish"
        if ! grep -Fqx -- "$fish_line" "$fish_profile" 2>/dev/null; then printf '\n%s\n' "$fish_line" >> "$fish_profile"; fi ;;
    esac
    for profile in "${profiles[@]}"; do
      if ! grep -Fqx -- "$path_line" "$profile" 2>/dev/null; then printf '\n# miy CLI\n%s\n' "$path_line" >> "$profile"; fi
    done
    echo 'Open a new terminal to use the updated PATH.'
  fi

)
miy_install_online "$@"
