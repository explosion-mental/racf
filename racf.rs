//! # `racf`
//! TODO thermal policies
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::Path;
use std::process::exit;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use getsys::Cpu::{perc, try_turbo, temp, TurboState};
use getsys::PerCpu;
use serde::Deserialize;
use thiserror::Error;
use owo_colors::{OwoColorize, AnsiColors};
use psutil::process::processes;

#[cfg(test)]
mod tests;

/// separates generic error mgs from original ones
static SP: &str = "\n    ";

/// Errors types to match against in main()
#[derive(Debug, Error)]
enum MainE {
    /// general I/O errors from battery crate
    #[error("Failed to fetch battery info:{SP}{0}")]
    Bat(#[from] battery::Error),

    // miscellaneous i/o errors
    #[error("An io error ocurred:{SP}{0}")]
    Io(#[from] io::Error),

    /// In case interpretting the `avgload` file fails, let's be safe. (probably overkill)
    #[error("Fetching from /proc/avgload failed:{SP}{0}")]
    Proc(String),

    #[error("Error while reading a file '{1}':{SP}{0}")]
    Read(#[source] io::Error, String),

    #[error("Error while writting a file '{1}':{SP}{0}")]
    Write(#[source] io::Error, String),

    /// Failed to deserialize the toml config file
    #[error("Config file: Failed to deserialize, make sure toml types are correct:{SP}{0}")]
    Deser(#[from] toml::de::Error),

    /// Missing config file
    #[error("Config file: Not found at '/etc/racf/config.toml'.")]
    MissingConfig,

    /// Not a valid governor, given your /sys fs
    #[error("Config file: governor '{0}' is invalid, use -l or --list to check avaliable governors.")]
    WrongGov(String),

    /// Not a valid frequency, given your /sys fs
    #[error("Config file: frequency '{0}' is invalid, use -l or --list to check avaliable frequencies.")]
    WrongFreq(u32),

    /// Already running
    #[error("Stopped. racf is already running at {0}.")]
    Running(u32),

    /// `.next()` returned `None`, meaning no battery found...
    #[error("Error: Could not find a battery in this device.")]
    NoBat,

    /// `processes()` failed
    #[error("Error: Could not get processes list:{SP}{0}")]
    PsUtil(#[from] psutil::Error),

    /// methods from `procesees()` failed: `Result<Process, ProcessError>`
    #[error("Error: Could not get pid/name of the processes list:{SP}{0}")]
    ProcErr(#[from] psutil::process::ProcessError),

    /// caused when you use `frequency` parameter in your `config.toml` but the governor is not userspace
    #[error("Config File: In order to use a `frequency` the governor requires to be 'userspace'.")]
    NoUserspace,
}

// XXX are devices without a battery (desktop) valid systems to use this?

/// Cli flags
// consider a cli flag to accept a config file
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Enables/disables turbo boost
    //NOTE true/false should be enough, but consider using more generic words like "on" and "off"
    #[arg(short, long)]
    turbo: Option<bool>,

    /// Runs once and exits
    #[arg(short, long)]
    run_once: bool,

    /// Sets a governor
    #[arg(short, long)]
    governor: Option<String>,

    /// Prints stats about the system that racf uses
    #[arg(short, long)]
    list: bool,
}

/// Configuration struct for serde + toml.
/// Two profiles for 2 w scenarios: using battery or charging
#[derive(Debug, Deserialize)]
struct Config {
    battery: Profile,
    ac: Profile,
}

/// Little enum to choose between governor and frequency stats
enum StatKind {
    Governor,
    Freq,
}

/// Possible values of turbo. When `config.toml` turbo parameters don't match these, `toml` will
/// generate an error with the line and the expected values.
/// (With these there is no need to `.validate()` to match these values)
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TurboKind {
    Always,
    Auto,
    Never,
}

/// Profile: can be for `[battery]` or `[ac]`
#[derive(Debug, Deserialize)]
#[serde(rename_all(serialize = "lowercase", deserialize = "lowercase"))]
struct Profile {
    /// turbo boost, can be: 'always' - 'auto' - 'never'
    turbo: TurboKind,
    /// interval in seconds
    interval: u32,
    /// minimum cpu percentage to enable turbo boost
    mincpu: f64,
    /// minimum temperature to enable turbo boost
    mintemp: u32,
    /// governor to use, avaliable ones with -l
    //TODO maybe do the same thing as with TurboKind, since there are only so many governors
    governor: String,
    /// frequency to use, only avaliable on `userspace`
    frequency: Option<u32>,
}

impl Profile {
    /// Validates the configuration file
    /// Checks if the parameters for `Profile` are correct
    //XXX restrict other parameters as well?
    pub fn validate(&self) -> Result<(), MainE> {
        let gov = self.governor.to_ascii_lowercase();
        check_govs(&gov)?;

        if let Some(s) = self.frequency {
            if gov != "userspace" {
                return Err(MainE::NoUserspace);
            }
            check_freq(s)?;
        }

        Ok(())
    }

    /// Main logic, changes the configuration to use depending on the charging state.
    /// The idea is to use turbo boost when the below parameters
    /// (cpu percentage, temperature and threshold) are met.
    // TODO should threshold be configurable?
    pub fn set(&self, cpuperc: f64, cpus: usize) -> Result<(), MainE> {
        let threshold: f64 = ((75 * cpus) / 100) as f64;

        set_stat(StatKind::Governor, &self.governor)?;
        if let Some(s) = self.frequency {
            set_stat(StatKind::Freq, &s.to_string())?;
        };

        if self.turbo == TurboKind::Never {
            turbo(false)?;
        }
        else if self.turbo == TurboKind::Always || avgload()? >= threshold || cpuperc >= self.mincpu || temp() >= self.mintemp
        {
            turbo(true)?;
        }

        Ok(())
    }
}

fn main() -> ExitCode {
    let Err(e) = try_main() else {
        return ExitCode::SUCCESS;
    };

    if let MainE::Io(e) = &e {
        if e.kind() == io::ErrorKind::PermissionDenied {
            eprintln!("You need read/write permissions in /sys:{SP}{e}");
            return ExitCode::FAILURE;
        }
    };

    eprintln!("{e}");
    ExitCode::FAILURE
}

/// fallible version of main
fn try_main() -> Result<(), MainE> {
    cli_flags()?; // all cli flags exit()

    {
        let ppid = std::process::id();
        let processes = processes()?;

        for p in processes {
            let p = p?;
            if p.name()? == "racf" && p.pid() != ppid {
                return Err(MainE::Running(p.pid()));
            }
        }
    }

    let conf = parse_conf()?;
    let cpus = num_cpus::get();
    let mut cpuperc = perc(Duration::from_millis(200)); //tmp fast value
    let man = battery::Manager::new()?;
    let bat = get_bat(&man)?;

    loop {
        run(&conf, cpuperc, &bat, cpus)?;
        cpuperc = perc(Duration::from_secs(
                if bat.state() == battery::State::Charging { conf.ac.interval.into() } else { conf.battery.interval.into() }
                )); //sleep
    }
}

fn run(conf: &Config, cpuperc: f64, b: &battery::Battery, cpus: usize) -> Result<(), MainE> {
    let conf = if b.state() == battery::State::Charging { &conf.ac } else { &conf.battery };
    conf.set(cpuperc, cpus)?;
    Ok(())
}

/// Update battery info and make sure it is not None
fn get_bat(man: &battery::Manager) -> Result<battery::Battery, MainE> {
    let mut btt = match man.batteries()?.next() {
        Some(bats) => bats,
        None => return Err(MainE::NoBat),
    }?;
    man.refresh(&mut btt)?; // update values
    Ok(btt)
}

/// toml + serde to get config values into structs
fn parse_conf() -> Result<Config, MainE> {
    let p1 = "/etc/racf/config.toml";
    let p2 = "/etc/racf.toml";
    let p3 = "/etc/racf/racf.toml";

    let p = if Path::new(p1).exists() {
        p1
    } else if Path::new(p2).exists() {
        p2
    } else if Path::new(p3).exists() {
        p3
    } else {
        return Err(MainE::MissingConfig);
    };

    let contents = std::fs::read_to_string(p)
       .map_err(|e| MainE::Read(e, p.to_string()))?;
    let file: Config = toml::from_str(&contents)?;
    file.battery.validate()?;
    file.ac.validate()?;
    Ok(file)
}

/// parse cli flags
fn cli_flags() -> Result<(), MainE> {
    let a = Cli::parse();

    if a.list {
        info()?;
        exit(0);
    } else if let Some(t) = a.turbo {
        turbo(t)?;
        exit(0);
    } else if a.run_once {
        let f = parse_conf()?;
        let man = battery::Manager::new()?;
        let bat = get_bat(&man)?;
        run(&f, perc(Duration::from_millis(200)), &bat, num_cpus::get())?;
        exit(0);
    } else if let Some(gov) = a.governor.as_deref() {
        check_govs(gov)?;
        set_stat(StatKind::Governor, gov)?;
        exit(0);
    }

    Ok(())
}

/// Prints stats about the system. '-l' or '--list'
fn info() -> Result<(), MainE> {
    let man = battery::Manager::new()?;
    let b = get_bat(&man)?;
    let vendor = b.vendor().unwrap_or("Could not get battery vendor.");
    let model = b.model().unwrap_or("Could not get battery model.");

    let turbocol = match try_turbo() {
        TurboState::On => AnsiColors::Green,
        TurboState::Off => AnsiColors::Red,
        TurboState::NotSupported => AnsiColors::Yellow,
    };

    let statecol = if b.state() == battery::State::Charging {
        AnsiColors::Green
    } else {
        AnsiColors::Red
    };

    // vectors of the percpu info
    let f = PerCpu::freq();
    let g = PerCpu::governor();
    let d = PerCpu::driver();

    // wrap vectors into one `.zip`ped
    let vals = g.iter().zip(d.iter()).zip(f.iter()).enumerate();

    //XXX is `String::with_capacity(vals.len())` and `push_str()` more performant?
    // collect formated strings
    let percpu_stats = vals.map(
        |(i, ((gov, driver), freq))|
            format!("CPU{}\t{}\t{}\t{}\n", i, gov, driver, freq)
        ).collect::<String>();


    println!(
"{} battery is {} ({})
Turbo boost is {}
Avaliable governors:{SP}{}
Avaliable {} frequencies:{SP}{}
Average temperature: {} °C
Average cpu percentage: {:.2}%
{}\t{}\t{}\t{} {}
{}",
    model.bold().blue(),
    b.state().color(statecol).bold(),
    vendor.italic(),
    try_turbo().color(turbocol).bold(),
    get_stat(StatKind::Governor)?.trim(),
    "userspace".italic(),
    get_stat(StatKind::Freq)?.trim(),
    temp(),
    perc(Duration::from_millis(100)),
    "Core".bold().underline().yellow(),
    "Governor".bold().underline().yellow(),
    "Scaling Driver".bold().underline().yellow(),
    "Frequency".bold().underline().yellow(),
    "(kHz)".italic().yellow(),
    percpu_stats.trim(),
    );

    Ok(())
}

/// Sets the turbo boost state for all cpus.
fn turbo(on: bool) -> Result<(), MainE> {
    // TODO refactor `intel_pstate` detection and list it in info()
    let intelpstate = "/sys/devices/system/cpu/intel_pstate/no_turbo";
    let cpufreq = "/sys/devices/system/cpu/cpufreq/boost";

    let turbopath = if Path::new(intelpstate).exists() {
        intelpstate
    } else if Path::new(cpufreq).exists() {
        cpufreq
    } else { /* turbo boost is not supported */
        //TODO breaking change would be a crash, let's just report an error for now.
        //FIXME wait for getsys v2
        //return Err(MainE::NoTurbo);
        eprintln!("Warning: Turbo boost is not supported");
        return Ok(());
    };

    /* change state of turbo boost */
    File::create(turbopath)?
        .write_all(if on { b"1" } else { b"0" })
        .map_err(|e| MainE::Write(e, turbopath.to_owned()))?;

    Ok(())
}

/// Get the load average from the file rather than the libc call.
fn avgload() -> Result<f64, MainE> {
    let mut firstline = String::new();
    let path = "/proc/loadavg";
    std::io::BufReader::new(File::open(path)?)
        .read_line(&mut firstline)
        .map_err(|e| MainE::Read(e, path.to_string()))?;
    let mut s = firstline.split_ascii_whitespace();

    let Some(min1) = s.next() else {
        return Err(MainE::Proc("could not find".to_owned()));
    };

    let min1: f64 = match min1.parse() {
        Ok(o) => o,
        Err(e) => return Err(MainE::Proc(
                format!("expecting a f64: {e}")
        )),
    };
    // let min5  = s.next().unwrap().parse::<f64>().unwrap();
    // let min15 = s.next().unwrap().parse::<f64>().unwrap();

    //[ min1, min5, min15 ]
    Ok(min1)
}

/// Verifies if the str slice provided is actually valid.
/// In the case it's invalid, the program should report it and exit,
/// given that /sys will reject any of those with OS 22 error "invalid argument".
fn check_govs(gov: &str) -> Result<(), MainE> {
    let found =
        get_stat(StatKind::Governor)?
            .split_ascii_whitespace()
            .any(|x| x == gov);

    if found {
        Ok(())
    } else {
        Err(MainE::WrongGov(gov.to_owned()))
    }
}

/// Checks for the frequency provided is present in /sys
fn check_freq(freq: u32) -> Result<(), MainE> {
    let found =
        get_stat(StatKind::Freq)?
            .split_ascii_whitespace()
            .any(|x| x == freq.to_string());

    if found {
        Ok(())
    } else {
        Err(MainE::WrongFreq(freq))
    }
}


//XXX maybe add these as methors for the struct like `config.governor.set()`, which under the hood
//just calls these general funcs

//TODO evaluate to use either `../cpuX/` or `../policyX/`
/// Gets either `Governor` or `Freq` stats
/// * `Freq` => Returns avaliable frequencies for the system in a `String`, These can be used **only** in the
///           `userspace` governor.
/// * `Governor` => Returns avaliable governors for the system in a `String`.
fn get_stat(stat: StatKind) -> Result<String, MainE> {
    let p = match stat {
        StatKind::Governor => "/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors",
        StatKind::Freq => "/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_frequencies",
    };
    std::fs::read_to_string(p).map_err(|e| MainE::Write(e, p.to_string()))
}

/// Sets either `Governor` or `Freq` stats
/// * `Governor` => Sets the governor for all cpus.
/// * `Freq` => write to `scaling_setspeed` of every cpu
fn set_stat(stat: StatKind, value: &str) -> Result<(), MainE> {
    let cpus = num_cpus::get();
    let suf = match stat {
        StatKind::Governor => "scaling_governor",
        StatKind::Freq => "scaling_setspeed",
    };

    for i in 0..cpus {
        let path = format!("/sys/devices/system/cpu/cpu{i}/cpufreq/{suf}");
        File::create(&path)?
            .write_all(value.as_bytes())
            .map_err(|e| MainE::Write(e, path.to_owned()))?;
    }

    Ok(())
}
