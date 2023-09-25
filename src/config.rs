//! # Configuration related

use std::path::Path;

use serde::Deserialize;
use std::fs::read_to_string;

use crate::MainE;
use crate::Profile;

/// Configuration struct for serde + toml.
/// Two profiles for 2 w scenarios: using battery or charging
#[derive(Debug, Deserialize)]
pub struct Config {
    battery: Profile,
    ac: Profile,
}

impl Config {
    /// Returns the relevant profile for use
    pub fn current(&self, bat: &battery::Battery) -> &Profile {
        if bat.state() == battery::State::Charging {
            &self.ac
        } else {
            &self.battery
        }
    }
    /// Validates the configuration file
    pub fn validate(&self) -> Result<(), MainE> {
        self.battery.check()?;
        self.ac.check()?;
        Ok(())
    }
}

/// toml + serde to get config values into structs
pub fn parse_conf() -> Result<Config, MainE> {
    let p1 = "/etc/racf.toml";
    let p2 = "/etc/racf/racf.toml";
    let p3 = "/etc/racf/config.toml";

    let p = if Path::new(p1).exists() {
        p1
    } else if Path::new(p2).exists() {
        p2
    } else if Path::new(p3).exists() {
        p3
    } else {
        return Err(MainE::MissingConfig);
    };

    let contents = read_to_string(p)
       .map_err(|e| MainE::Read(e, p.to_owned()))?;
    let file: Config = toml::from_str(&contents)?;
    file.validate()?;
    Ok(file)
}


#[cfg(test)]
mod tests {
use super::*;
///! some tests to keep an eye on `racf` functionality over time

// TODO macro for a sample config file
// TODO even tho it's a binary, take a more modular approach, like using lib.rs. That way, here
// it's simple as importing that module instead of main()

/// Checks the [racf.toml](/racf.toml) of the repo
#[test]
fn check_config() {
    let contents = std::fs::read_to_string("./racf.toml").expect("racf.toml is always present in the repo");
    let f: Result<Config, toml::de::Error> = toml::from_str(&contents);

    if f.is_err() { // toml error
        dbg!(&f);
        panic!("\nThere is an issue with deserializing with TOML `racf.toml`:\n-->'{:?}'\n\n", f);
    }

    let f = f.expect("statement above checks for err").validate();

    if f.is_err() { // error with one parameter (turbo or governor)
        dbg!(&f);
        panic!("\nThere is an issue with validating `racf.toml`:\n-->'{:?}'\n\n", f);
    }
}

/// tests if `racf` can detect invalid governor parameters (for your system)
#[test]
fn invalid_governor() {
    let file: Config = toml::from_str(
"
[ac]
turbo = 'auto'
mincpu = 30.0
mintemp = 70
interval = 60
governor = 'performance'
[battery]
turbo = 'auto'
mincpu = 30.0
mintemp = 70
interval = 60
governor = \"erformance\" ") // <-- should be "performance"
        .expect("NeverFailing");

    let f = file.validate();
    if f.is_ok() { // Should error out when parsing `erformance`
        dbg!(&f);
        panic!("\nParsed an invalid governor.\n-->'{:?}'\n\n", f);
    };
}

/// tests if `racf` can detect invalid turbo boost parameters which are of `TurboKind`
#[test]
fn invalid_turbo() {
    let f: Result<Config, toml::de::Error> = toml::from_str(
// turbo values should be 'auto' - 'always' - 'never'
"
[ac]
turbo = \"aut\"
mincpu = 30.0
mintemp = 70
interval = 60
governor = 'performance'
[battery]
turbo = 'auto'
mincpu = 30.0
mintemp = 70
interval = 60
governor = 'performance' ");

    if f.is_ok() { // `serde` should error out when parsing `aut`
        dbg!(&f);
        panic!("\nParsed an invalid turbo value and returned no errors.\n-->'{:?}'\n\n", f);
    };
}
}
