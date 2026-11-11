# Rust (the distro's if new enough, else rustup into ~/.cargo) and the build of the shell and of each
# chosen module into the cache. Nothing reaches ~/.local/bin until every build has passed: then all
# binaries are swapped in one step (do_install_bins), so a failed module cannot leave a new shell with
# old modules. A module whose source has not changed since it was installed is not rebuilt. The login
# screen's binary goes to /usr/local/bin (login.sh).
# shellcheck shell=bash

plan_build() {
  if ((PREBUILT)); then
    add "$(t "ставлю готовое" "installing the ready binaries" "instalando os binários prontos")" do_prebuilt_bins
    add "$(t "ставлю собранное" "installing what was built" "instalando o que foi compilado")" do_install_bins
    return 0
  fi
  ((APPLY)) || add "$(t "шелл: сборка из исходников" "shell: build from source" "shell: compilar do código-fonte")" do_build_shell
  # каждый модуль — своя строка: видно, что сейчас собирается
  local id
  for id in files view play shot; do
    on "$id" && add "$(t "сборка:" "build:" "compilar:") yami-$id" do_build_module "$id"
  done
  wants login && add "$(t "сборка:" "build:" "compilar:") yami-greet" do_build_greet
  add "$(t "ставлю собранное" "installing what was built" "instalando o que foi compilado")" do_install_bins
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

# the commit of a source tree; nothing for a tree with edits (never counted as unchanged)
src_rev() { [[ -z "$(git -C "$1" status --porcelain --untracked-files=no 2>/dev/null)" ]] && git -C "$1" rev-parse HEAD 2>/dev/null; }
# this exact source is what is installed: no need to build it again (a repair rebuilds anyway)
src_installed() { # binary source-tree
  local rev
  ((!REPAIR)) && [[ -x $BIN/$1 ]] && rev="$(src_rev "$2")" && [[ -n $rev && "$(cat "$STATE/built/$1" 2>/dev/null)" == "$rev" ]]
}
# built and waiting for do_install_bins: the commit it came from, or «-» for a tree with edits
built_mark() { # binary source-tree
  local rev
  rev="$(src_rev "$2")"
  printf '%s\n' "${rev:--}" >"$CACHE/built-$1"
}
do_build_shell() {
  rm -f "$CACHE/built-yami-shell" "$CACHE/built-yami-agents"
  # the services (yami-agents) are part of the shell: built from the same tree, with it
  src_installed yami-shell "$ROOT" && src_installed yami-agents "$ROOT" && return 99
  need_rust || return 1
  cargo_build "$ROOT/shell/Cargo.toml" -p yami-shell -p yami-agents || return 1
  built_mark yami-shell "$ROOT"
  built_mark yami-agents "$ROOT"
}
do_build_module() {
  local repo src
  repo="$(m_repo "$1")" || return 1
  rm -f "$CACHE/built-$repo"
  src="$(m_src "$repo")" || return 1
  src_installed "$repo" "$src" && return 99
  need_rust || return 1
  cargo_build "$src/Cargo.toml" || return 1
  built_mark "$repo" "$src"
}
# every build has passed: all at once. A record per binary says which source it came from
do_install_bins() {
  local b rev any=0
  for b in yami-shell yami-files yami-view yami-play yami-shot yami-agents; do
    [[ -f $CACHE/built-$b ]] || continue
    put_bin "$b" || return 1
    any=1
  done
  ((any)) || return 99
  # ready binaries: nothing worth keeping in the cache (yami-greet is taken after this step, login.sh)
  ((PREBUILT)) && for b in yami-shell yami-files yami-view yami-play yami-shot yami-agents; do rm -f "$CACHE/target/release/$b"; done
  # the separate setup program of the development tree is part of the shell now (`yami setup`)
  rm -f "$BIN/yami-setup" "$STATE/built/yami-setup"
  # `yami <command>`: the same binary, help instead of the shell when started without a command
  [[ -x $BIN/yami-shell ]] && ln -sfn yami-shell "$BIN/yami"
  mkdir -p "$STATE/built"
  for b in yami-shell yami-files yami-view yami-play yami-shot yami-agents; do
    [[ -f $CACHE/built-$b ]] || continue
    rev="$(cat "$CACHE/built-$b")"
    if [[ $rev == - ]]; then rm -f "$STATE/built/$b"; else printf '%s\n' "$rev" >"$STATE/built/$b"; fi
    rm -f "$CACHE/built-$b"
  done
}
do_build_greet() {
  need_rust || return 1
  local src
  src="$(m_src yami-greet)" || return 1
  cargo_build "$src/Cargo.toml"
}
