use std::process::exit;
use std::time::Duration;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use getsys::{Cpu, PerCpu};
use num_cpus;
use serde::Deserialize;
use std::io;
use thiserror::Error;
use clap::Parser;

/* macros */
macro_rules! die {
    ($fmt:expr) => ({ print!(concat!($fmt, "\n")); std::process::exit(1) });
    ($fmt:expr, $($arg:tt)*) => ({ print!(concat!($fmt, "\n"), $($arg)*); std::process::exit(1) });
}

// XXX thiserror overkill?
#[derive(Debug, Error)]
enum MainE {
    /// The config file doesn't exist
    #[error("Battery")]
    Bat(#[from] battery::Error),

    #[error(transparent)]
    Io(#[from] io::Error),

    /// Failed to deserialize the toml config file
    #[error("Failed to deserialize config file")]
    Deser(#[from] toml::de::Error),

    /// Wrong parameter of some kind
    #[error("Config: parameter '{found}' is invalid, expected: '{expected}'.")]
    WrongArg {
        expected: String,
        found: String,
    },
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

#[derive(Debug, Deserialize)]
struct Config {
    battery: BatConfig,
    ac: BatConfig,
}

#[derive(Debug, Deserialize)]
struct BatConfig {
    turbo: String,
    interval: u32,
    mincpu: f64,
    mintemp: u32,
    governor: String,
}

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
        // TODO only allow for avaliable governors, current
        // impl is generic governors (most systems should have it)
        let gov = c.governor.to_ascii_lowercase();

        if !(gov == "conservative"
        || gov == "ondemand"
        || gov == "userspace"
        || gov == "powersafe"
        || gov == "performance"
        || gov == "schedutil")
        {
            //errors.push(
            return Err(
                MainE::WrongArg { expected: "governor".to_string(), found: c.governor.to_owned() }
                );
        }

        Ok(())
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
    match setup() {
        Ok(()) => (),
        Err(e) => die!("{}", e),
    }

    let conf = match parse_conf() {
        Ok(o) => o,
        Err(MainE::Io(e)) if e.kind() == io::ErrorKind::NotFound => die!("Error: configuration file doesn't exist: {}",  e),
        Err(MainE::Deser(e)) => die!("Failed to deserialize config file: {}", e),
        Err(e) => die!("{}", e),
    };

    let cpus = num_cpus::get();
    let mut cpuperc = Cpu::perc(std::time::Duration::from_millis(200)); //init val

    loop {
        match run(&conf, cpuperc, cpus) {
            Ok(()) => (),
            Err(MainE::Bat(e)) => die!("Error reading battery values: {}", e),
            Err(MainE::Io(ref e)) if e.kind() == io::ErrorKind::PermissionDenied => die!("Error: You don't have read and write permissions on /sys: {}", e),
            Err(e) => die!("{}", e),
        }
        //TODO sleep
        cpuperc = Cpu::perc(Duration::from_secs(conf.ac.interval.into())); //sleep
    }
}

fn parse_conf() -> Result<Config, MainE> {
    let contents = std::fs::read_to_string("test.toml")?;
    let file: Config = toml::from_str(&contents)?;
    match file.validate() {
        Ok(()) => (),
        Err(e) => die!("{}", e),
    }
    Ok(file)
}

fn setup() -> Result<(), MainE> {
    // Cli args
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
        run(&f, Cpu::perc(Duration::from_millis(200)), num_cpus::get())?;
    } else if let Some(gov) = a.governor.as_deref() {
        setgovernor(gov)?;
        exit(0);
    }

    Ok(())
}

//TODO battery::Manager::new() in main() and pass it to this fn
fn run(conf: &Config, cpuperc: f64, cpus: usize) -> Result<(), MainE> {
    let man = battery::Manager::new()?;
	let threshold: f64 = ((75 * cpus) / 100) as f64;
    let btt = man.batteries()?.next().unwrap();
    let charging = if btt?.state() == battery::State::Charging { true } else { false };
    let conf = if charging { &conf.ac } else { &conf.battery };

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

fn info() -> Result<(), MainE> {
    println!("Turbo boost is {}",
             if Cpu::turbo() == true { "enabled" } else { "disabled" }
             );
    println!("Average temperature: {} °C", Cpu::temp());
    println!("Average cpu percentage: {:.2}%",
             Cpu::perc(std::time::Duration::from_millis(200))
             );

    let manager = battery::Manager::new()?;
    for (idx, maybe_battery) in manager.batteries()?.enumerate() {
        let b = maybe_battery?;
        println!("Using battery #{}:", idx);
        println!("\tVendor: {}", b.vendor().unwrap());
        println!("\tModel: {}", b.model().unwrap());
        println!("\tState: {}", b.state());
        break;
    }

    println!("");

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
    fp.write_all(on.to_string().as_bytes())?;
    Ok(())
}

fn setgovernor(gov: &str) -> Result<(), MainE> {
    let cpus = num_cpus::get();

    for i in 0..cpus {
        let mut fp = File::create(
            format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", i)
            )?;
        fp.write_all(gov.as_bytes())?;
	}
    Ok(())
}

//fn avgload() -> [f64; 3] {
fn avgload() -> Result<f64, MainE> {
        let mut firstline = String::new();
        let mut buffer = std::io::BufReader::new(
                    File::open("/proc/loadavg")?
                    );
        buffer.read_line(&mut firstline)?;
        let mut s = firstline.split_ascii_whitespace();
        let min1  = s.next().unwrap().parse::<f64>().unwrap();
       // let min5  = s.next().unwrap().parse::<f64>().unwrap();
       // let min15 = s.next().unwrap().parse::<f64>().unwrap();

        //[ min1, min5, min15 ]
        Ok(min1)
}
