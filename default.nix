{
  src,
  naersk,
  pkgs,
  release ? false,
}:
naersk.buildPackage {
  name = "flume";
  inherit src;
  # naersk adds `--release` itself when this is set.
  inherit release;
  doCheck = false;
  meta.mainProgram = "flume";
}
