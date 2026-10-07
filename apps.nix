{
  pkgs,
  toolchain,
}: {
  fix = {
    type = "app";
    program = "${pkgs.writeShellApplication {
      name = "cargo-fix";
      runtimeInputs = [toolchain];
      text = ''
        cargo clippy --fix --allow-dirty --allow-staged --all-targets
      '';
    }}/bin/cargo-fix";
    meta = {
      description = "apply linter suggestions";
    };
  };
}
