{
  pkgs ? import <nixpkgs> { overlays = [ (import <rust-overlay>) ]; }
}:
let
  toolchain = pkgs.rust-bin.nightly.latest.default.override {
    targets = [ "x86_64-unknown-linux-gnu" ];
    extensions = [ "rust-src" "rust-analyzer" "clippy" ];
  };
in
pkgs.mkShell {
  nativeBuildInputs = [
    toolchain
    pkgs.pkg-config
    pkgs.nodejs
    pkgs.opus
    pkgs.cmake
  ];
}
