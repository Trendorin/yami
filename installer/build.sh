# Rust (the distro's if new enough, else rustup into ~/.cargo) and the build of
# the shell and the chosen apps (yami-files, yami-view, yami-play, yami-greet) into the cache, then
# the binaries into ~/.local/bin, each swapped in whole.
# shellcheck shell=bash

plan_build() {
  ((APPLY)) || add "$(t "шелл: сборка из исходников" "shell: build from source" "shell: compilar do código-fonte")" do_build_shell
  # каждый модуль — своя строка: видно, что сейчас собирается
  local id
  for id in files view play shot agents; do
    on "$id" && add "$(t "сборка:" "build:" "compilar:") yami-$id" do_build_module "$id"
  done
  return 0
}

ver_ge() { [[ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n1)" == "$2" ]]; }
rust_ok() {
  local v
  v="$(rustc --version 2>/dev/null | awk '{print $2}')"
  [[ -n $v ]] && ver_ge "$v" "$RUST_MIN" && command -v cargo >/dev/null
}
need_rust() {
  [[ -d $HOME/.cargo/bin ]] && export PATH="$HOME/.cargo/bin:$PATH"
  rust_ok && return 0
  [[ $FAM == nixos ]] && return 0 # nix-shell provides it
  if command -v rustup >/dev/null; then
    rustup toolchain install stable --profile minimal && rustup default stable
  else
    echo "rustc older than $RUST_MIN or missing: installing rustup into ~/.cargo"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --no-modify-path
  fi
  export PATH="$HOME/.cargo/bin:$PATH"
  rust_ok
}
# cargo with the settings of this machine; the common target folder is shared by the shell and modules
cargo_build() {
  # из папки проекта: там его .cargo/config.toml (локальные версии библиотек при разработке)
  local dir="${1%/*}" cmd=(cargo build --release --locked --target-dir "$CACHE/target" "${@:2}")
  # a weak machine: thin LTO in 16 units instead of fat LTO in one. Measured with two jobs: the
  # rustc peak is 1.3 GB instead of 2.4 GB (fat LTO links each binary with all its crates at once),
  # so 4 GB does not swap; the binaries are ~20 % larger, the speed difference is not visible
  ((LIGHT)) && cmd=(env CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 "${cmd[@]}")
  if [[ $FAM == nixos ]] && ! rust_ok; then
    (cd "$dir" && nix-shell -p cargo rustc pkg-config libxkbcommon pam --run "${cmd[*]}")
  else
    (cd "$dir" && "${cmd[@]}")
  fi
}
# a copy next to it, then a rename: the running program keeps its old file, and a full disk or a cut
# copy leaves the previous binary in place, never half of a new one
put_bin() { install -Dm755 "$CACHE/target/release/$1" "$BIN/.$1.new" && mv -f "$BIN/.$1.new" "$BIN/$1" || { rm -f "$BIN/.$1.new"; return 1; }; }

do_build_shell() {
  need_rust || return 1
  cargo_build "$ROOT/shell/Cargo.toml" -p yami-shell -p yami-setup || return 1
  put_bin yami-shell && put_bin yami-setup || return 1
  # `yami <command>`: the same binary, help instead of the shell when started without a command
  ln -sfn yami-shell "$BIN/yami"
}
do_build_module() {
  need_rust || return 1
  local repo src
  repo="$(m_repo "$1")" || return 1
  src="$(m_fetch "$repo")" || return 1
  cargo_build "$src/Cargo.toml" || return 1
  put_bin "$repo" || return 1
  printf '%s
' "$src" >"$CACHE/src-$repo"
}
