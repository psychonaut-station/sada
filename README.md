# sada

Voice chat integration for Space Station 13.

## Building from Source

This repository includes a `shell.nix` for a reproducible development shell.

To set up the nix environment, add the following channels:

```sh
nix-channel --add https://nixos.org/channels/nixos-unstable nixpkgs # In case you don't have it already
nix-channel --add https://github.com/oxalica/rust-overlay/archive/master.tar.gz rust-overlay
nix-channel --update
```

Then enter the shell and build the Rust workspace:

```sh
nix-shell
cargo build
```

Ideally, you may want to run your editor in the nix shell as well, so the language server can pick up the dependencies.

## sada-client

A bridge library between game server and VC server. Uses byondapi to take load off the BYOND.

## sada-common

Shared utilities library. Contains packet protocols, etc...

## sada-server

Main VC server. Voice routing and auth happens here. Mixer/voice processing planned.

## sada-web

Frontend implementation that connects to server.
