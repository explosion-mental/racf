use std::process::exit;
use std::time::Duration;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use getsys::{Cpu, PerCpu};
use num_cpus;
use std::error::Error;
use serde::Deserialize;

//mod args;

use clap::Parser;

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

/* macros */
macro_rules! die {
    ($fmt:expr) => ({ print!(concat!($fmt, "\n")); std::process::exit(1) });
    ($fmt:expr, $($arg:tt)*) => ({ print!(concat!($fmt, "\n"), $($arg)*); std::process::exit(1) });
}

fn main() -> Result<(), battery::Error> {
    let a = Cli::parse();

    println!("{:?}", a);

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
        todo!("the run function");
    } else if let Some(gov) = a.governor.as_deref() {
        setgovernor(gov)?;
        exit(0);
    }

    let contents = std::fs::read_to_string("test.toml")?;
    let file: Config = toml::from_str(&contents).unwrap();
    // println!("{:#?}", decoded);
    println!("file: {:?}", file);
    println!("battery governor: {}", file.ac.governor);

    let man = battery::Manager::new()?;
    let cpus = num_cpus::get();
	let threshold: f64 = ((75 * cpus) / 100) as f64;
    let mut cpuperc = Cpu::perc(std::time::Duration::from_millis(200));

	loop {
        let btt = man.batteries()?.next().unwrap();
        let charging = if btt?.state() == battery::State::Charging { true } else { false };
        let conf = if charging { &file.ac } else { &file.battery };
        let gov = if charging { &conf.governor } else { &conf.governor };
        let tb  = if charging { conf.turbo.to_ascii_lowercase() } else { conf.turbo.to_ascii_lowercase() };

        setgovernor(&gov)?;
        if tb == "never" {
            turbo(0)?;
        }
        else if tb == "always" || avgload()? >= threshold || cpuperc >= conf.mincpu || Cpu::temp() >= conf.mintemp
        {
            turbo(1)?;
        }
        cpuperc = Cpu::perc(Duration::from_secs(conf.interval.into()));
    }
}

fn info() -> Result<(), battery::Error> {
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

fn turbo(on: i8) -> std::io::Result<()> {
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

fn setgovernor(gov: &str) -> std::io::Result<()> {
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
fn avgload() -> std::io::Result<f64> {
        let mut firstline = String::new();
        let mut buffer = std::io::BufReader::new(
                    File::open("/proc/loadavg").unwrap()
                    );
        buffer.read_line(&mut firstline)?;
        let mut s = firstline.split_ascii_whitespace();
        let min1  = s.next().unwrap().parse::<f64>().unwrap();
       // let min5  = s.next().unwrap().parse::<f64>().unwrap();
       // let min15 = s.next().unwrap().parse::<f64>().unwrap();

        //[ min1, min5, min15 ]
        Ok(min1)
}
