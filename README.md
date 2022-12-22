racf - simple auto cpu frequencer (IN RUST)
===========================================
a rewrite of [sacf](https://github.com/explosion-mental/sacf) in rust.

Wait for version 1.0.0

Building and Installing
-----------------------
Currently you need to build it from source (not that big) with cargo
and then, optionally, move it to your PATH. In the example bellow I use
`/usr/local/bin/` as the PREFIX (target) directory.

```
$ cargo build --release
# cp ./target/release/racf /usr/local/bin/
```

Configuration
-------------
This repo contains [config.toml](./config.toml) configuration example
with the respective documentation for it's parameters.
First create `/etc/racf` directory, then you can move or copy the config in that dir.
Note that on most systems you will need root to write to `/etc`

```
# mkdir /etc/racf
# cp ./config.toml /etc/racf/config.toml
```

crates
------
- battery = "0.7.8"
- clap = { version = "4.0.29", features = ["derive"] }
- serde = { version = "1.0.150", features = ["derive"] }
- num_cpus = "1.14.0"
- toml = "0.5.10"
- thiserror = "1.0.38"
- getsys = { git = "https://github.com/explosion-mental/getsys" } - This is my own library that complements this project
