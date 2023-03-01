use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::Path;
use std::process::exit;
use std::time::Duration;

use clap::Parser;
use getsys::{Cpu, PerCpu};
use serde::Deserialize;
use thiserror::Error;
use sysinfo::{ProcessExt, System, SystemExt, get_current_pid}; //XXX check temperature with sysinfo¿
use std::process::ExitCode;

#[cfg(test)]
mod tests;

static SP: &str = "\n    "; // separates generic error mgs from original ones

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

    /// Wrong turbo boost parameter
    #[error("Config file: turbo as '{0}' is invalid, expected: 'always', 'never' or 'auto'.")]
    WrongTurbo(String),

    /// Already running
    #[error("Stopped. racf is already running at {0}.")]
    Running(String),

    /// Could not get the parent pid of this proc
    #[error("Couldn't get process pid:{SP}{0}")]
    Pid(String),

    /// `.next()` returned `None`, meaning no battery found...
    #[error("Error: Could not find a battery in this device.")]
    NoBat,
}

// XXX are devices without a battery (desktop) valid systems to use this?

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

/// Profile: can be for `[battery]` or `[ac]`
#[derive(Debug, Deserialize)]
struct Profile {
    /// turbo boost, can be: 'always' - 'auto' - 'never'
    turbo: String,
    /// interval in seconds
    interval: u32,
    /// minimum cpu percentage to enable turbo boost
    mincpu: f64,
    /// minimum temperature to enable turbo boost
    mintemp: u32,
    /// governor to use, avaliable ones with -l
    governor: String,
}

/// Little struct to hold useful values about the battery.
/// This is easier and simpler than to use battery::Battery struct
struct BatInfo {
    charging: bool,
    vendor: String,
    model: String,
}

impl Config {
    /// Validates the configuration file
    // XXX return a vec<> of errors of the whole file, instead of returning early
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
        } else {
            eprintln!("{e}");
        }
        return ExitCode::FAILURE;
    };

    eprintln!("{e}");
    ExitCode::FAILURE
}


fn try_main() -> Result<(), MainE> {
    cli_flags()?; // all cli flags exit()

    {// Check if racf is already running after parsing the clip flags
        let s = System::new_all();
        let ppid = match get_current_pid() {
            Ok(o) => o,
            Err(e) => return Err(MainE::Pid(e.to_string())),
        };

        for process in s.processes_by_exact_name("racf") {
            if process.pid() != 0.into() && process.pid() != ppid {
                return Err(MainE::Running(process.pid().to_string()));
            }
        }
    }

    let conf = parse_conf()?;
    let cpus = num_cpus::get();
    let mut cpuperc = Cpu::perc(std::time::Duration::from_millis(200)); //tmp fast value
    let man = battery::Manager::new()?;
    let bat = get_bat(&man)?;

    loop {
        run(&conf, cpuperc, &bat, cpus)?;
        cpuperc = Cpu::perc(Duration::from_secs(
                if bat.charging { conf.ac.interval.into() } else { conf.battery.interval.into() }
                )); //sleep
    }
}

/// Main logic, changes the configuration to use depending on the charging state.
/// The idea is to use turbo boost when the below parameters
/// (cpu percentage, temperature and threshold) are met.
fn run(conf: &Config, cpuperc: f64, b: &BatInfo, cpus: usize) -> Result<(), MainE> {
    // TODO should threshold be configurable?
    let threshold: f64 = ((75 * cpus) / 100) as f64;
    let conf = if b.charging { &conf.ac } else { &conf.battery };

    setgovernor(&conf.governor)?;
    if conf.turbo == "never" {
        turbo(0)?;
    }
    else if conf.turbo == "always" || avgload()? >= threshold || cpuperc >= conf.mincpu || Cpu::temp() >= conf.mintemp
    {
        turbo(1)?;
    }

    Ok(())
}

/// Checks if the parameters for `Profile` are correct
fn validate_conf(c: &Profile) -> Result<(), MainE> {
    // Check turbo
    let tb = c.turbo.to_ascii_lowercase();

    if !(tb == "always" || tb == "never" || tb == "auto") {
        //errors.push(
        return Err(
            MainE::WrongTurbo(c.turbo.to_string())
        );
    }

    // Check governor
    let gov = c.governor.to_ascii_lowercase();
    check_govs(&gov)?;
    //XXX restrict other parameters as well?

    Ok(())
}

/// Simpler interface to battery crate, this fills a BatInfo struct
fn get_bat(man: &battery::Manager) -> Result<BatInfo, MainE> {
    let mut btt = man.batteries()?;

    // shadow original value, which will not be used
    let mut btt = match btt.next() {
        Some(bats) => bats,
        None => return Err(MainE::NoBat),
    }?;

    // update values
    man.refresh(&mut btt)?;

    Ok(BatInfo {
        charging: btt.state() == battery::State::Charging,
        // the two fields below are not vital for the main logic, but it's used in info().
        // with that in mind, we can ignore these.
        // TODO avoid this and move it into info() entirely.
        vendor: btt.vendor().unwrap_or("Could not get battery vendor.").to_string(),
        model: btt.model().unwrap_or("Could not get battery model.").to_string(),
    })
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

fn cli_flags() -> Result<(), MainE> {
    // XXX cli flag to pass a config file¿
    let a = Cli::parse();

    if a.list {
        info()?;
        exit(0);
    } else if let Some(t) = a.turbo {
        match t {
            true => turbo(1)?,
            false => turbo(0)?,
        }
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
    print!("Using battery:");
    print!("{SP}Vendor: {}", b.vendor);
    print!("{SP}Model: {}", b.model);
    print!("{SP}State: {}", if b.charging { "Charging" } else { "Disconected" });

    println!("Turbo boost is {}",
             if Cpu::turbo() { "enabled" } else { "disabled" });

    print!("Avaliable governors:{SP}{}", get_govs()?);

    println!("Average temperature: {} °C", Cpu::temp());
    println!("Average cpu percentage: {:.2}%",
             Cpu::perc(std::time::Duration::from_millis(100))
             );

    /* get vector of values */
    let freq = PerCpu::freq();
    let gov  = PerCpu::governor();
    let driv = PerCpu::driver();

    let mut f = freq.iter();
    let mut g = gov.iter();
    let mut d = driv.iter();

    println!("Core\tGovernor\tScaling Driver\tFrequency(kHz)");
    for i in 0..freq.len() {
        println!("CPU{}\t{}\t{}\t{}", i,
                 g.next().unwrap_or(&"err".to_string()),
                 d.next().unwrap_or(&"err".to_string()),
                 f.next().unwrap_or(&"err".to_string()),
                 );
    }
    Ok(())
}

/// Sets the turbo boost state for all cpus.
fn turbo(on: i8) -> Result<(), MainE> {
    // TODO refactor `intel_pstate` detection and list it in info()
    let turbopath;
    let intelpstate = "/sys/devices/system/cpu/intel_pstate/no_turbo";
    let cpufreq = "/sys/devices/system/cpu/cpufreq/boost";

    if Path::new(intelpstate).exists() {
        turbopath = intelpstate;
    } else if Path::new(cpufreq).exists() {
        turbopath = cpufreq;
    } else { /* turbo boost is not supported */
        return Ok(()); /* TODO show error output */
    }

    /* change state of turbo boost */
    let mut fp = File::create(turbopath)?;
    fp.write_all(on.to_string().as_bytes()).map_err(MainE::Write)?;
    Ok(())
}

/// Sets the governor for all cpus.
fn setgovernor(gov: &str) -> Result<(), MainE> {
    let cpus = num_cpus::get();

    for i in 0..cpus {
        let mut fp = File::create(
            format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", i)
            )?;
        fp.write_all(gov.as_bytes()).map_err(MainE::Write)?;
	}
    Ok(())
}

/// Get the load average from the file rather than the libc call.
fn avgload() -> Result<f64, MainE> {
        let mut firstline = String::new();
        let mut buffer = std::io::BufReader::new(
                    File::open("/proc/loadavg")?
                    );
        buffer.read_line(&mut firstline).map_err(MainE::Read)?;
        let mut s = firstline.split_ascii_whitespace();
        let min1  = match s.next() {
            Some(s) => s,
            None => return Err(MainE::Proc("could not find".to_string())),
        };
        let min1 = match min1.parse::<f64>() {
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
        get_govs()?
            .split_ascii_whitespace()
            .any(|x| x == gov);

    if found {
        Ok(())
    } else {
        Err(MainE::WrongGov(gov.to_string()))
    }
}

/// Returns avaliable governors for the system in a `String`.
fn get_govs() -> Result<String, MainE> {
    //XXX should be the same for all cpus
    let p = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors";
    std::fs::read_to_string(p).map_err(MainE::Read)
}
