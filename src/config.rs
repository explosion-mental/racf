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

