# yami: the shell and its modules, each built from its own Cargo.lock (the modules are flake inputs, their
# versions follow modules.lock). Updates belong to Nix: the `package-managed` feature turns the built-in
# updater off. Fonts and the per-user configs go to share/, every module's .desktop, portal and D-Bus
# files next to it, the login screen's session script to libexec/yami (it finds bin/yami-greet from there).
{
  lib,
  rustPlatform,
  symlinkJoin,
  pkg-config,
  libxkbcommon,
  linux-pam,
  wayland,
  mpv-unwrapped,
  version,
  src,
  modules,
}:

let
  # each Cargo.lock pins its crates; the modules take the drawing library yami-px by a tag of this
  # repository (px-vX.Y.Z), the shell has it in its own tree (shell/px)
  lockOf = dir: {
    lockFile = "${dir}/Cargo.lock";
    allowBuiltinFetchGit = true;
  };
  common = {
    inherit version;
    # the tests draw panels offscreen with the user's fonts and settings: tools/check.sh runs them
    doCheck = false;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [
      libxkbcommon
      wayland
    ];
    meta = {
      homepage = "https://github.com/Trendorin/yami";
      license = lib.licenses.mit;
      platforms = lib.platforms.linux;
    };
  };

  shell = rustPlatform.buildRustPackage (
    common
    // {
      pname = "yami-shell";
      inherit src;
      cargoRoot = "shell";
      buildAndTestSubdir = "shell";
      cargoLock = lockOf "${src}/shell";
      buildFeatures = [ "yami-shell/package-managed" ];
      # the shell and its services (polkit window, screen sharing, automount) from the same workspace
      cargoBuildFlags = [
        "-p"
        "yami-shell"
        "-p"
        "yami-agents"
      ];
      buildInputs = common.buildInputs ++ [ linux-pam ];
      postInstall = ''
        # `yami <command>`: the same binary, help instead of the shell without a command
        ln -s yami-shell $out/bin/yami
        install -Dm644 config/autostart/yami-shell.desktop $out/etc/xdg/autostart/yami-shell.desktop
        # per-user configs the Home Manager module links or seeds into ~/.config
        install -Dm644 -t $out/share/yami config/niri/config.kdl config/kitty/yami.conf \
          config/portal/niri-portals.conf config/fontconfig/50-yami-pixel.conf
        install -Dm644 -t $out/share/fonts/truetype/yami fonts/*.ttf
        install -Dm644 -t $out/share/licenses/yami fonts/OFL-*.txt LICENSE
        install -Dm644 shell/agents/config/yami-agents.portal $out/share/xdg-desktop-portal/portals/yami-agents.portal
      ''
      + service "shell/agents/config" "org.freedesktop.impl.portal.desktop.yamiagents.service" "yami-agents";
      meta = common.meta // {
        description = "Lightweight pixel-art Wayland shell for the niri compositor";
        mainProgram = "yami";
      };
    }
  );

  # a module: its binary and the files of its config/ folder
  module =
    name: extra:
    rustPlatform.buildRustPackage (
      common
      // {
        pname = name;
        src = modules.${name};
        cargoLock = lockOf modules.${name};
      }
      // extra
    );

  desktop = name: ''
    install -Dm644 config/${name}.desktop $out/share/applications/${name}.desktop
  '';
  # a D-Bus service file: @BIN@ is the binary that serves it
  service = dir: name: bin: ''
    install -Dm644 ${dir}/${name} $out/share/dbus-1/services/${name}
    substituteInPlace $out/share/dbus-1/services/${name} --replace-fail @BIN@ $out/bin/${bin}
  '';

  files = module "yami-files" {
    postInstall =
      desktop "yami-files"
      + ''
        install -Dm644 config/yami.portal $out/share/xdg-desktop-portal/portals/yami.portal
      ''
      + service "config" "org.freedesktop.impl.portal.desktop.yami.service" "yami-files"
      + service "config" "org.freedesktop.FileManager1.service" "yami-files";
  };
  view = module "yami-view" { postInstall = desktop "yami-view"; };
  play = module "yami-play" {
    postInstall = desktop "yami-play";
    # yami-play loads libmpv with dlopen; the binary's RUNPATH lets it find the store copy
    postFixup = ''
      patchelf --add-rpath ${lib.makeLibraryPath [ mpv-unwrapped ]} $out/bin/yami-play
    '';
  };
  shot = module "yami-shot" { };
  greet = module "yami-greet" {
    postInstall = ''
      # the login screen: its session script and its two fonts
      install -Dm755 greet-session $out/libexec/yami/greet-session
      install -Dm644 -t $out/share/yami-greet/fonts fonts/Tiny5.ttf fonts/DotGothic16-Regular.ttf
    '';
  };
in
symlinkJoin {
  name = "yami-${version}";
  paths = [
    shell
    files
    view
    play
    shot
    greet
  ];
  meta = shell.meta;
}
