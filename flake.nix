{
  description = "yami: a lightweight pixel-art Wayland desktop for the niri compositor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # the modules live in their own repositories; the versions are those of modules.lock (the shell's
    # services, yami-agents, and the drawing library yami-px are part of this repository)
    yami-files = {
      url = "github:Trendorin/yami-files/v3.0.0";
      flake = false;
    };
    yami-view = {
      url = "github:Trendorin/yami-view/v3.0.0";
      flake = false;
    };
    yami-play = {
      url = "github:Trendorin/yami-play/v3.0.0";
      flake = false;
    };
    yami-shot = {
      url = "github:Trendorin/yami-shot/v3.0.0";
      flake = false;
    };
    yami-greet = {
      url = "github:Trendorin/yami-greet/v3.0.0";
      flake = false;
    };
  };

  outputs =
    { self, nixpkgs, ... }@inputs:
    let
      inherit (nixpkgs) lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAll = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      version = lib.fileContents ./VERSION;
      # only what the build reads: a change to docs, CI or the installer does not rebuild yami. keys/: the
      # release keys are compiled into the shell (update.rs), so a check never trusts a file it is handed
      src = lib.fileset.toSource {
        root = ./.;
        fileset = lib.fileset.unions [
          ./LICENSE
          ./VERSION
          ./config
          ./fonts
          ./keys
          (lib.fileset.difference ./shell (lib.fileset.maybeMissing ./shell/target))
        ];
      };
      modules = lib.getAttrs [
        "yami-files"
        "yami-view"
        "yami-play"
        "yami-shot"
        "yami-greet"
      ] inputs;
      packagesFor = pkgs: {
        yami = pkgs.callPackage ./nix/package.nix { inherit version src modules; };
      };
    in
    {
      packages = forAll (
        pkgs:
        packagesFor pkgs
        // {
          default = self.packages.${pkgs.stdenv.hostPlatform.system}.yami;
        }
      );

      # `nix flake check`: the package builds, the NixOS module evaluates (greetd runs yami-greet, the
      # portals, services and tools are set)
      checks = forAll (
        pkgs:
        let
          system = pkgs.stdenv.hostPlatform.system;
          c =
            (nixpkgs.lib.nixosSystem {
              modules = [
                self.nixosModules.default
                {
                  nixpkgs.hostPlatform = system;
                  programs.yami = {
                    enable = true;
                    loginScreen.user = "yami";
                  };
                  users.users.yami.isNormalUser = true;
                  fileSystems."/" = {
                    device = "/dev/null";
                    fsType = "ext4";
                  };
                  boot.loader.grub.enable = false;
                  system.stateVersion = "25.11";
                }
              ];
            }).config;
        in
        {
          inherit (self.packages.${system}) yami;
          nixos-module = pkgs.writeText "yami-nixos-module.json" (
            builtins.toJSON {
              greeter = c.services.greetd.settings.default_session.command;
              tmpfiles = c.systemd.tmpfiles.rules;
              portals = map (p: p.name) c.xdg.portal.extraPortals;
              packages = map (p: p.name) c.environment.systemPackages;
            }
          );
        }
      );

      nixosModules.default = import ./nix/nixos-module.nix self;
      homeManagerModules.default = import ./nix/hm-module.nix self;

      overlays.default = _final: prev: packagesFor prev;

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.yami ];
          packages = with pkgs; [
            cargo
            rustc
            clippy
            shellcheck
          ];
          # yami-play finds libmpv at run time
          LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.mpv-unwrapped ];
        };
      });

      formatter = forAll (pkgs: pkgs.nixfmt);
    };
}
