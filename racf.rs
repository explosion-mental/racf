use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::Path;
use std::process::exit;
use std::time::Duration;

use clap::Parser;
use getsys::{Cpu, PerCpu};
use num_cpus;
use serde::Deserialize;
use thiserror::Error;
use sysinfo::{ProcessExt, System, SystemExt, get_current_pid}; //XXX check temperature with sysinfo¿

/* macros */
macro_rules! die {
    ($fmt:expr) => ({ print!(concat!($fmt, "\n")); std::process::exit(1) });
    ($fmt:expr, $($arg:tt)*) => ({ print!(concat!($fmt, "\n"), $($arg)*); std::process::exit(1) });
}

/// Errors types to match against in main()
#[derive(Debug, Error)]
enum MainE {
    /// I/O errors from battery crate
    #[error(transparent)]
    Bat(#[from] battery::Error),

    // miscellaneous i/o errors
    #[error(transparent)]
    Io(#[from] io::Error),

    /// In case interpretting the `avgload` file fails, let's be safe. (probably overkill)
    #[error("Fetching from /proc/avgload failed: {warn}")]
    Proc { warn: String },

    /// Failed to deserialize the toml config file
    #[error("Failed to deserialize config file, make sure toml types are correct.")]
    Deser(#[from] toml::de::Error),

    /// Failed to deserialize the toml config file
    #[error("Governor '{found}' is invalid, use -l or --list to check avaliable governors.")]
    WrongGov { found: String },

    /// Missing config file
    #[error("Config file not found at '/etc/racf/config.toml'.")]
    MissingConfig,

    /// Wrong parameter of some kind
    #[error("Config parameter '{found}' is invalid, expected: '{expected}'.")]
    WrongArg {
        found: String,
        expected: String,
    },

    #[error("Error while reading a file: {0}")]
    Read(#[source] io::Error),

    #[error("Error while writting a file: {0}")]
    Write(#[source] io::Error),
}

#[derive(Parser, Debug)]
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
    battery: BatConfig,
    ac: BatConfig,
}

#[derive(Debug, Deserialize)]
struct BatConfig {
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

fn main() {
    match try_main() {
        Ok(()) => (),
        Err(MainE::MissingConfig) => die!("{}", MainE::MissingConfig),
        Err(MainE::Read(e)) => die!("{}", e), //read_to_string()
        Err(MainE::Write(e)) => die!("{}", e), //write_all()
        Err(MainE::Io(e)) => match e.kind() {
            io::ErrorKind::PermissionDenied => die!("Error: You don't have read and write permissions on /sys: {}", e),
            _ => die!("{}", e),
        }
        Err(MainE::Deser(e)) => die!("Failed to deserialize config file: {}", e),
        Err(MainE::Bat(e)) => die!("Error fetching battery values: {}", e),
        Err(MainE::Proc { warn }) => die!("{}", warn),
        Err(MainE::WrongGov { found }) => die!("{}", MainE::WrongGov { found: found }),
        Err(MainE::WrongArg { found, expected }) => die!("{}", MainE::WrongArg { found: found, expected: expected }),
    };
}


fn try_main() -> Result<(), MainE> {
    cli_flags()?;

    {// Check if racf is already running after parsing the clip flags (all of those exit())
        let s = System::new_all();
        let ppid = match get_current_pid() {
            Ok(o) => o,
            Err(e) => die!("Failed to get pid: {}", e),
        };

        for process in s.processes_by_exact_name("racf") {
            if process.pid() != 0.into() && process.pid() != ppid {
                die!("racf is already running ({}).", process.pid());
            }
        }
    }

    let conf = parse_conf()?;
    let cpus = num_cpus::get();
    let mut cpuperc = Cpu::perc(std::time::Duration::from_millis(200)); //tmp fast value
    let man = battery::Manager::new()?;
    let bat = get_bat(&man);

    loop {
        run(&conf, cpuperc, &bat, cpus)?;
        cpuperc = Cpu::perc(Duration::from_secs(
                if bat.charging { conf.ac.interval.into() } else { conf.battery.interval.into() }
                )); //sleep
    }
}

/// Checks if the parameters for BatConfig are correct
fn validate_conf(c: &BatConfig) -> Result<(), MainE> {
        // Check turbo
        let tb = c.turbo.to_ascii_lowercase();

        if !(tb == "always"
        || tb == "never"
        || tb == "auto")
        {
            //errors.push(
            return Err(
                MainE::WrongArg { expected: "always, never, auto".to_string(), found: c.turbo.to_owned() }
                );
        }

        // Check governor
        let gov = c.governor.to_ascii_lowercase();
        check_govs(&gov)?;
        //XXX restrict other parameters as well?

        Ok(())
}

/// Simpler interface to battery crate, this fills a BatInfo struct
fn get_bat(man: &battery::Manager) -> BatInfo {

    let mut btt = match man.batteries() {
        Ok(o) => o,
        Err(e) => die!("Error getting batteries: {}", e),
    };

    let mut btt = match btt.next() {
        Some(bats) => match bats {
            Ok(o) => o,
            Err(e) => die!("{}", e),
        },
        None => die!("Could not fetch information about the battery"),
    };

    // update values
    match man.refresh(&mut btt) {
        Ok(()) => (),
        Err(e) => die!("Error updating battery: {}", e),
    };

    let state = if btt.state() == battery::State::Charging { true } else { false };

    // This information is not vital for the main logic, but it's used in info().
    // That in mind, we can ignore these.
    let vendor = match btt.vendor() {
        Some(s) => s,
        None => "Could not get battery vendor."
    };
    let model  = match btt.model() {
        Some(s) => s,
        None => "Could not get battery model."
    };

    BatInfo {
        charging: state,
        vendor: vendor.to_string(),
        model: model.to_string(),
    }
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
        let bat = get_bat(&man);
        run(&f, Cpu::perc(Duration::from_millis(200)), &bat, num_cpus::get())?;
        exit(0);
    } else if let Some(gov) = a.governor.as_deref() {
        check_govs(gov)?;
        setgovernor(gov)?;
        exit(0);
    }

    Ok(())
}

/// Main logic, changes the configuration to use depending on the charging state.
/// The idea is to use turbo boost when the below parameters
/// (cpu percentage, temperature and threshold) are met.
fn run(conf: &Config, cpuperc: f64, b: &BatInfo, cpus: usize) -> Result<(), MainE> {
// TODO what about threshold¿
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

/// Prints stats about the system. '-l' or '--list'
fn info() -> Result<(), MainE> {

    let man = battery::Manager::new()?;
    let b = get_bat(&man);
    println!("Using battery");
    println!("\tVendor: {}", b.vendor);
    println!("\tModel: {}", b.model);
    println!("\tState: {}", if b.charging { "Charging" } else { "Disconected" });

    println!("Turbo boost is {}",
             if Cpu::turbo() == true { "enabled" } else { "disabled" }
             );

    let p = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors";
    let contents = std::fs::read_to_string(p).map_err(MainE::Read)?;
    let g = contents.split_ascii_whitespace();
    print!("Avaliable governors:\n\t");
    for i in g {
        print!("{} ", i);
    }
    println!("");
    println!("Average temperature: {} °C", Cpu::temp());
    println!("Average cpu percentage: {:.2}%",
             Cpu::perc(std::time::Duration::from_millis(200))
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
                 g.next().unwrap(),
                 d.next().unwrap(),
                 f.next().unwrap(),
                 );
    }
    Ok(())
}

/// Sets the turbo boost state for all cpus.
fn turbo(on: i8) -> Result<(), MainE> {
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
            None => return Err(MainE::Proc { warn: "could not find".to_string() }),
        };
        let min1 = match min1.parse::<f64>() {
            Ok(o) => o,
            Err(_e) => return Err(MainE::Proc { warn: "expecting a f64: {e}".to_string() } ),
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
    //XXX should be the same for all cpus
    let p = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors";
    let contents = std::fs::read_to_string(p).map_err(MainE::Read)?;
    let g = contents.split_ascii_whitespace();
    let mut found = false;

    for i in g {
        if gov == i {
            found = true;
            break;
        }
    }

    if found {
        Ok(())
    } else {
        Err(MainE::WrongGov { found: gov.to_string() })
    }
}
