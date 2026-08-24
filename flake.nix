{
  description = "FerrisPad development environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      forAllSystems = f:
        nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ]
          (system: f nixpkgs.legacyPackages.${system});
    in {
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          name = "ferrispad-dev";

          # FLTK's C++ build breaks under nixpkgs' default hardening flags.
          hardeningDisable = [ "all" ];

          buildInputs = with pkgs; [
            rustc cargo rustfmt clippy
            fontconfig pango cairo
            libx11 libxext libxinerama libxcursor libxrender libxfixes libxft
            wayland libxkbcommon dbus
          ];

          nativeBuildInputs = with pkgs; [
            stdenv.cc cmake pkg-config gnumake
          ];

          shellHook = ''
            export CMAKE_PREFIX_PATH="${pkgs.pango.dev}:${pkgs.cairo.dev}:${pkgs.fontconfig.dev}"
            echo "🦀 FerrisPad dev environment loaded 🦀"
          '';
        };
      });
    };
}
