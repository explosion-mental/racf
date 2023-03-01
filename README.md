racf - auto cpu frequencer
==========================
[![CodeBerg](https://img.shields.io/badge/Hosted_at-Codeberg-%232185D0?style=flat-square&logo=CodeBerg)](https://codeberg.org/explosion-mental/racf)
[![license](https://img.shields.io/badge/license-GPL--3.0-lightgreen?style=flat-square)](./LICENSE)
[![loc](https://img.shields.io/tokei/lines/github/explosion-mental/racf?color=lightgreen&style=flat-square)](./racf.rs)
<br>

Simple and configurable tool that dynamically switches turbo boost and the
kernel governor in order to have a corresponding relationship between the
computer's capabilities and the actual usage.


A rewrite of [sacf](https://github.com/explosion-mental/sacf) in rust.

Building and Installing
-----------------------
Currently you need to build it from source (not that big) with cargo
and then, optionally, move it to your PATH. In the example bellow I use
`/usr/local/bin/` as the PREFIX (target) directory.

```sh
cargo build --release
cp -f ./target/release/racf /usr/local/bin/
```
Alternatively use `cargo install racf`

Configuration
-------------
This repo contains [config.toml](./config.toml) configuration example
with the respective documentation for it's parameters.
First create `/etc/racf` directory, then you can move or copy the config in that dir.
Note that on most systems you will need root to write to `/etc`

You can copy the config file when building with:
```sh
mkdir -p /etc/racf
cp -f config.toml /etc/racf/config.toml
```

or simply copy and paste it.
