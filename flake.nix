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

      # The tests compare against the compiler in `..`, which is outside this
      # flake and so missing in the build sandbox. Cargo reads every path
      # dependency, dev or not, so drop it here; only the tests need it.
      src = pkgs.runCommand "flume-src" {} ''
        cp -r ${self} $out
        chmod -R +w $out
        sed -i '/^rill = { path = "\.\."/d' $out/Cargo.toml
      '';

      flume = naerskLib.buildPackage {
        pname = "flume";
        inherit src;
        doCheck = false;
        meta.mainProgram = "flume";
      };
    in {
      packages.default = flume;

      apps.default = flake-utils.lib.mkApp {drv = flume;};

      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          toolchain
          nixd
          alejandra
        ];
        env.RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
      };

      formatter = pkgs.alejandra;
    });
}
