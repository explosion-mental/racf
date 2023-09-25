//! # `racf`
//! * TODO thermal policies
//! * TODO think about handling devices without a battery (desktop)
//!
//! `grep` out the source code for some
use std::fs::File;
use std::fs::read_to_string;
use std::io;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::process::exit;
use std::time::Duration;

use serde::Deserialize;
use getsys::Cpu::{perc, try_turbo, temp, TurboState};
use getsys::PerCpu;
use thiserror::Error;
use owo_colors::{OwoColorize, AnsiColors};
use psutil::process::processes;
use clap::Parser;

//#[cfg(test)]
//mod tests;
mod config;
mod args;

/// Used to separate generic error mgs from original ones, if any
static SP: &str = "\n    ";

/// Errors types to match against in main()
#[derive(Debug, Error)]
pub enum MainE {
    /// general I/O and misc errors from battery crate
    #[error("Failed to fetch battery info:{SP}{0}")]
    Bat(#[from] battery::Error),

    /// miscellaneous i/o errors
    //TODO shouldn't need this
    #[error("An io error ocurred:{SP}{0}")]
    Io(#[from] io::Error),

    /// In case interpretting the `avgload` file fails, let's be safe. (probably overkill)
    #[error("Fetching from /proc/avgload failed:{SP}{0}")]
    Proc(String),

    /// `map_err()` when `read_to_string`
    /// 0: source error
    /// 1: path of the error file
    #[error("Error while reading a file {1}:{SP}{0}")]
    Read(#[source] io::Error, String),

    /// `map_err()` when `.create` and `write_all`
    /// 0: source error
    /// 1: path of the error file or an unusual but simple error, see [`set_stat`]
    #[error("Error while writting a file {1}:{SP}{0}")]
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

    /// Running more than **one instance** should not be avaliable
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
pub struct Profile {
    /// turbo boost, can be: 'always' - 'auto' - 'never'
    turbo: TurboKind,
    /// interval in seconds
    interval: u32,
    /// minimum cpu percentage to enable turbo boost
    mincpu: f64,
    /// minimum temperature to enable turbo boost
    mintemp: u32,
    /// governor to use, avaliable ones with -l
    //TODO maybe do the same thing as with TurboKind, since there are only so many governors,
    //     requires checking if all (most) kernels support the same governors
    governor: String,
    /// frequency to use, only avaliable on `userspace`
    frequency: Option<u32>,
}

impl Profile {
    /// Checks if the parameters for `Profile` are correct
    //XXX restrict other parameters as well?
    pub fn check(&self) -> Result<(), MainE> {
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
    pub fn run(&self, cpuperc: f64, cpus: usize) -> Result<(), MainE> {
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

    eprintln!("{e}");
    ExitCode::FAILURE
}

/// Fallible version of main
fn try_main() -> Result<(), MainE> {
    cli_flags()?; // all cli flags exit()

    // TODO create/delete a lock file instead, given that you can be running `racf -l` while racf is running.
    // XXX the above requires write/read permissions for /var, which is the usual place for this kind of files
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

    let conf = config::parse_conf()?;
    let cpus = num_cpus::get();
    let mut cpuperc = perc(Duration::from_millis(200)); //tmp fast value
    let man = battery::Manager::new()?;
    let bat = get_bat(&man)?;

    loop {
        let current_profile = conf.current(&bat);
        current_profile.run(cpuperc, cpus)?;
        cpuperc = perc(Duration::from_secs(current_profile.interval.into())); //sleep
    }
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

    // change state of turbo boost
    File::create(turbopath)
        .map_err(|e| MainE::Write(e, turbopath.to_owned()))?
        .write_all(if on { b"1" } else { b"0" })
        .map_err(|e| MainE::Write(e, turbopath.to_owned()))?;

    Ok(())
}

/// Get the load average from the file rather than the libc call.
/// cat /proc/loadavg: 1.17 0.96 0.94 1/1069 8782
///                    ^    ^    ^
///                   /     5    |
///        1 min ----+     mins  +----- 15 mins
fn avgload() -> Result<f64, MainE> {
    let p = "/proc/loadavg";
    let loadavg = read_to_string(p).map_err(|e| MainE::Write(e, p.to_owned()))?;
    let mut s = loadavg.split_ascii_whitespace();

    let Some(min1) = s.next() else {
        return Err(MainE::Proc("could not find".to_owned()));
    };

    let min1: f64 = min1.parse()
        .map_err(|e| MainE::Proc(format!("Error when parsing a string, expected f64:{SP}{e}")))?;

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
    read_to_string(p).map_err(|e| MainE::Write(e, p.to_owned()))
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
        File::create(&path)
            .map_err(|e| {
                let msg = if e.kind() == io::ErrorKind::PermissionDenied {
                    "- you need read/write permissions in /sys"
                } else {
                    &path
                };
                MainE::Write(e, msg.to_owned())
            })?
            .write_all(value.as_bytes())
            .map_err(|e| MainE::Write(e, path.to_owned()))?;
    }

    Ok(())
}

/// Parse cli flags with clap
fn cli_flags() -> Result<(), MainE> {
    let a = args::Cli::parse();

    if a.list {
        info()?;
        exit(0);
    } else if let Some(t) = a.turbo {
        turbo(t)?;
        exit(0);
    } else if a.run_once {
        let conf = config::parse_conf()?;
        let man = battery::Manager::new()?;
        let bat = get_bat(&man)?;
        conf.current(&bat).run(perc(Duration::from_millis(200)), num_cpus::get())?;
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

    // collect formated strings
    let percpu_stats = vals.map(
        |(i, ((gov, driver), freq))|
            format!("CPU{}\t{}\t{}\t{}\n", i, gov, driver, freq)
        ).collect::<String>();
    //XXX is `String::with_capacity(vals.len())` and `push_str()` more performant?


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
