racf - simple auto cpu frequencer (IN RUST)
===========================================
[![license](https://img.shields.io/badge/license-GPL--3.0-lightgreen?style=flat-square)](./LICENSE)
[![loc](https://img.shields.io/tokei/lines/github/explosion-mental/racf?color=lightgreen&style=flat-square)](./racf.rs)
<br>
a rewrite of [sacf](https://github.com/explosion-mental/sacf) in rust.

Wait for version 1.0.0


Building and Installing
-----------------------
Currently you need to build it from source (not that big) with cargo
and then, optionally, move it to your PATH. In the example bellow I use
`/usr/local/bin/` as the PREFIX (target) directory.

```sh
cargo build --release
cp -f ./target/release/racf /usr/local/bin/
```
Alternatively use `cargo install`

Configuration
-------------
This repo contains [config.toml](./config.toml) configuration example
with the respective documentation for it's parameters.
First create `/etc/racf` directory, then you can move or copy the config in that dir.
Note that on most systems you will need root to write to `/etc`

Copy the config file:
```sh
mkdir -p /etc/racf
cp -f config.toml /etc/racf/config.toml
```

Crates
------
* num_cpus = "1.14.0"
* battery = { version = "0.7.*", package = "starship-battery" } - using starship
* getsys = { git = "https://codeberg.org/explosion-mental/getsys", version = "1.0.0" } - my custom lib
* clap = { version = "4.0.29", features = ["derive"] }
* toml = "0.5.10"
* serde = { version = "1.0.150", features = ["derive"] }
* thiserror = "1.0.38"
* sysinfo = { version = "0.27.7", default-features = false }
