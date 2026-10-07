{
  src,
  naersk,
  pkgs,
  toolchain,
}: {
  cargo-check = naersk.buildPackage {
    inherit src;
    mode = "check";
  };

  # Only the unit tests: the integration tests in `tests/` need the compiler
  # in `..`, so they run with `cargo test` in the repository instead.
  cargo-test = naersk.buildPackage {
    inherit src;
    mode = "test";
    cargoTestOptions = opts: opts ++ ["--lib" "--bins"];
  };

  cargo-clippy = naersk.buildPackage {
    inherit src;
    mode = "clippy";
  };

  cargo-fmt =
    pkgs.runCommand "cargo-fmt-check" {
      buildInputs = [toolchain];
    } ''
      cp -r ${src} ./source
      chmod -R +w ./source
      cd ./source
      cargo fmt --all -- --check
      touch $out
    '';
}
