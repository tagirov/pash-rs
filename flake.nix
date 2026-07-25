{
  description = "pash-rs - simple password manager, a dependency-free Rust port of pash";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems
        (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        pash-rs = pkgs.rustPlatform.buildRustPackage {
          pname = "pash-rs";
          version = "1.1.0";

          src = self;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [ pkgs.makeWrapper ];

          # gpg and fzf are spawned at runtime; make sure they are
          # always reachable. The clipboard tool is intentionally left
          # to the system (configurable via PASH_CLIP).
          postFixup = ''
            wrapProgram $out/bin/pash \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.gnupg pkgs.fzf ]}
          '';

          meta = with pkgs.lib; {
            description = "Simple password manager - Rust port of pash";
            license = licenses.mit;
            mainProgram = "pash";
            platforms = platforms.linux;
          };
        };
        default = pash-rs;
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc rustfmt clippy ];
        };
      });
    };
}
