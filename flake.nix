{
  description = "flume is the formatter for the rill audio programming language";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    naersk,
    fenix,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {inherit system;};
      fenixLib = fenix.packages.${system};
      toolchain = with fenixLib;
        combine [
          (stable.withComponents [
            "rustc"
            "cargo"
            "rustfmt"
            "clippy"
            "rust-src"
            "rust-docs"
            "rust-std"
            "rust-analyzer"
          ])
        ];

      naerskLib = (pkgs.callPackage naersk {}).override {
        cargo = toolchain;
        rustc = toolchain;
      };

      # The integration tests compare against the compiler in `..`, which is
      # outside this flake and so missing in the build sandbox. Cargo reads
      # every path dependency, dev or not, so drop it; nothing else needs it.
      src = pkgs.runCommand "flume-src" {} ''
        cp -r ${self} $out
        chmod -R +w $out
        sed -i '/^rill = { path = "\.\."/d' $out/Cargo.toml
      '';

      flumePackage = {release}:
        import ./default.nix {
          naersk = naerskLib;
          inherit src;
          inherit pkgs;
          inherit release;
        };

      checks = import ./checks.nix {
        naersk = naerskLib;
        inherit src;
        inherit pkgs;
        inherit toolchain;
      };

      apps = import ./apps.nix {
        inherit pkgs;
        inherit toolchain;
      };
    in {
      packages = rec {
        default = debug;
        debug = flumePackage {release = false;};
        release = flumePackage {release = true;};
      };

      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          toolchain
          nixd
          alejandra
        ];
        env.RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
      };

      checks = checks;

      apps = apps;

      formatter = pkgs.writeShellApplication {
        name = "fmt";
        runtimeInputs = [toolchain];
        text = "cargo fmt --all";
      };
    });
}
