racf - auto cpu frequencer
==========================
[![CodeBerg](https://img.shields.io/badge/Hosted_at-Codeberg-%232185D0?style=flat-square&logo=CodeBerg)](https://codeberg.org/explosion-mental/racf)
[![license](https://img.shields.io/badge/license-GPL--3.0-lightgreen?style=flat-square)](./LICENSE)
[![loc](https://img.shields.io/tokei/lines/github/explosion-mental/racf?color=lightgreen&style=flat-square)](./racf.rs)
[![cratesv](https://img.shields.io/crates/v/racf?style=flat-square&color=red)](https://crates.io/crates/racf)
<br>

Simple and configurable tool that dynamically switches turbo boost and the
kernel governor in order to have a corresponding relationship between the
computer's capabilities and the actual usage.


Another important variable is whether the machine is charging or using the
battery, depending on this state `racf` will use the corresponding
configuration profile.


This is intended mainly for battery based machines like laptops. Desktops
**could** benefit, I haven't really thought about it that much (e.g. those
systems would only be on the '[ac]' profile).


A rewrite of [sacf](https://github.com/explosion-mental/sacf) in rust.


Reference: [cpufreq](https://www.kernel.org/doc/html/v4.14/admin-guide/pm/cpufreq.html)

Usage
-----

```sh
racf --help
```

**Note** A very helpful flag is `--run-once` which, runs once; and thus no need
for `racf` to stay in the background. This way you can manually tweak your
system with the help of `racf` whenever you actually need it (might be useful
to put this in the status bar).

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
This repo contains [racf.toml](./racf.toml) configuration example
with the respective documentation for it's parameters.

`racf` searches config files in:
1. `/etc/racf.toml`
2. `/etc/racf/racf.toml`
3. `/etc/racf/config.toml`

The first config file that is found is used.


You can copy the file like so:
```sh
cp -f racf.toml /etc/racf.toml
```
or simply copy and paste it.
