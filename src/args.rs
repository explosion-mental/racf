//! # Command line options

use clap::Parser;

/// Cli flags
// consider a cli flag to accept a config file
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Enables/disables turbo boost
    //NOTE true/false should be enough, but consider using more generic words like "on" and "off"
    #[arg(short, long)]
    pub turbo: Option<bool>,

    /// Runs once and exits
    #[arg(short, long)]
    pub run_once: bool,

    /// Sets a governor
    #[arg(short, long)]
    pub governor: Option<String>,

    /// Prints stats about the system that racf uses
    #[arg(short, long)]
    pub list: bool,
}
