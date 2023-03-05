//! # `racf`
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::Path;
use std::process::exit;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use getsys::{Cpu, PerCpu};
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

    #[error("Error while reading a file:{SP}{0}")]
    Read(#[source] io::Error),

    #[error("Error while writting a file:{SP}{0}")]
    Write(#[source] io::Error),

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

impl Config {
    /// Validates the configuration file
    pub fn validate(&self) -> Result<(), MainE> {
        validate_conf(&self.battery)?;
        validate_conf(&self.ac)?;
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
    let mut cpuperc = Cpu::perc(Duration::from_millis(200)); //tmp fast value
    let man = battery::Manager::new()?;
    let bat = get_bat(&man)?;

    loop {
        run(&conf, cpuperc, &bat, cpus)?;
        cpuperc = Cpu::perc(Duration::from_secs(
                if bat.state() == battery::State::Charging { conf.ac.interval.into() } else { conf.battery.interval.into() }
                )); //sleep
    }
}

/// Main logic, changes the configuration to use depending on the charging state.
/// The idea is to use turbo boost when the below parameters
/// (cpu percentage, temperature and threshold) are met.
fn run(conf: &Config, cpuperc: f64, b: &battery::Battery, cpus: usize) -> Result<(), MainE> {
    // TODO should threshold be configurable?
    let threshold: f64 = ((75 * cpus) / 100) as f64;
    let conf = if b.state() == battery::State::Charging { &conf.ac } else { &conf.battery };

    setgovernor(&conf.governor)?;
    if let Some(s) = conf.frequency {
        setfrequency(s)?;
    };
    if conf.turbo == TurboKind::Never {
        turbo(false)?;
    }
    else if conf.turbo == TurboKind::Always || avgload()? >= threshold || cpuperc >= conf.mincpu || Cpu::temp() >= conf.mintemp
    {
        turbo(true)?;
    }

    Ok(())
}

/// Checks if the parameters for `Profile` are correct
fn validate_conf(c: &Profile) -> Result<(), MainE> {
    //XXX restrict other parameters as well?
    let gov = c.governor.to_ascii_lowercase();
    check_govs(&gov)?;

    if let Some(s) = c.frequency {
        if gov != "userspace" {
            return Err(MainE::NoUserspace);
        }
        check_freq(s)?;
    }

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
    let p = "/etc/racf/config.toml";
    if ! Path::new(p).exists() {
        return Err(MainE::MissingConfig);
    }
    let contents = std::fs::read_to_string(p).map_err(MainE::Read)?;
    let file: Config = toml::from_str(&contents)?;
    file.validate()?;
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
        run(&f, Cpu::perc(Duration::from_millis(200)), &bat, num_cpus::get())?;
        exit(0);
    } else if let Some(gov) = a.governor.as_deref() {
        check_govs(gov)?;
        setgovernor(gov)?;
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

    let mut turbocol = AnsiColors::Green;
    let turbo = if Cpu::turbo() {
        "enabled".color(turbocol)
    } else {
        turbocol = AnsiColors::Red;
        "disabled".color(turbocol)
    };

    let mut statecol = AnsiColors::Green;
    let state = if b.state() == battery::State::Charging {
        "Charging".color(statecol)
    } else {
        statecol = AnsiColors::Red;
        "Disconected".color(statecol)
    };

    println!(
"{} battery is {} ({})
Turbo boost is {}
Avaliable governors:{SP}{}
Avaliable {} frequencies:{SP}{}
Average temperature: {} °C
Average cpu percentage: {:.2}%
{}\t{}\t{}\t{} {}",
    model.bold().blue(),
    state.bold(),
    vendor.italic(),
    turbo.bold(),
    get_govs()?.trim(),
    "userspace".italic(),
    get_freq()?.trim(),
    Cpu::temp(),
    Cpu::perc(Duration::from_millis(100)),
    "Core".bold().underline().yellow(),
    "Governor".bold().underline().yellow(),
    "Scaling Driver".bold().underline().yellow(),
    "Frequency".bold().underline().yellow(),
    "(kHz)".italic().yellow(),
    );

    /* get vector of values */
    let f = PerCpu::freq();
    let g = PerCpu::governor();
    let d = PerCpu::driver();

    let sz = f.len();

    let mut f = f.iter();
    let mut g = g.iter();
    let mut d = d.iter();

    for i in 0..sz {
        println!("CPU{}\t{}\t{}\t{}", i,
                 g.next().unwrap_or(&"err".to_owned()),
                 d.next().unwrap_or(&"err".to_owned()),
                 f.next().unwrap_or(&"err".to_owned()),
                 );
    }
    Ok(())
}

/// Sets the turbo boost state for all cpus.
fn turbo(on: bool) -> Result<(), MainE> {
    // TODO refactor `intel_pstate` detection and list it in info()
    let turbopath;
    let intelpstate = "/sys/devices/system/cpu/intel_pstate/no_turbo";
    let cpufreq = "/sys/devices/system/cpu/cpufreq/boost";

    if Path::new(intelpstate).exists() {
        turbopath = intelpstate;
    } else if Path::new(cpufreq).exists() {
        turbopath = cpufreq;
    } else { /* turbo boost is not supported */
        //TODO breaking change would be a crash, let's just report an error for now.
        //FIXME wait for getsys v2
        //return Err(MainE::NoTurbo);
        eprintln!("Warning: Turbo boost is not supported");
        return Ok(());
    }

    /* change state of turbo boost */
    File::create(turbopath)?
        .write_all(if on { b"1" } else { b"0" })
        .map_err(MainE::Write)?;

    Ok(())
}

/// Sets the governor for all cpus.
fn setgovernor(gov: &str) -> Result<(), MainE> {
    let cpus = num_cpus::get();

    for i in 0..cpus {
        File::create(
            format!("/sys/devices/system/cpu/cpu{i}/cpufreq/scaling_governor")
            )?
            .write_all(gov.as_bytes())
            .map_err(MainE::Write)?;
    }

    Ok(())
}

/// Get the load average from the file rather than the libc call.
fn avgload() -> Result<f64, MainE> {
    let mut firstline = String::new();
    std::io::BufReader::new(File::open("/proc/loadavg")?)
        .read_line(&mut firstline)
        .map_err(MainE::Read)?;
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

/// write to `scaling_setspeed` of every cpu
fn setfrequency(freq: u32) -> Result<(), MainE> {
    let cpus = num_cpus::get();

    for i in 0..cpus {
        File::create(
            format!("/sys/devices/system/cpu/cpu{i}/cpufreq/scaling_setspeed")
            )?
            .write_all(freq.to_string().as_bytes())
            .map_err(MainE::Write)?;
    }

    Ok(())
}

/// Verifies if the str slice provided is actually valid.
/// In the case it's invalid, the program should report it and exit,
/// given that /sys will reject any of those with OS 22 error "invalid argument".
fn check_govs(gov: &str) -> Result<(), MainE> {
    let found =
        get_govs()?
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
        get_freq()?
            .split_ascii_whitespace()
            .any(|x| x == freq.to_string());

    if found {
        Ok(())
    } else {
        Err(MainE::WrongFreq(freq))
    }
}

//TODO evaluate between `../cpuX/` and `../policyX/`

/// Returns avaliable governors for the system in a `String`.
fn get_govs() -> Result<String, MainE> {
    let p = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors";
    std::fs::read_to_string(p).map_err(MainE::Read)
}

/// Returns avaliable frequencies for the system in a `String`, These can be used **only** in the
/// `userspace` governor.
fn get_freq() -> Result<String, MainE> {
    let p = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_frequencies";
    std::fs::read_to_string(p).map_err(MainE::Read)
}
