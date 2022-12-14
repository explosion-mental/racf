use std::env;
use std::process::exit;
use std::time::Duration;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use getsys::{Cpu, PerCpu};
use num_cpus;

/* macros */
macro_rules! die {
    ($fmt:expr) => ({ print!(concat!($fmt, "\n")); std::process::exit(1) });
    ($fmt:expr, $($arg:tt)*) => ({ print!(concat!($fmt, "\n"), $($arg)*); std::process::exit(1) });
}

fn main() {
    let argv: Vec<String> = env::args().collect();
    let argc = argv.len();

	for i in 1..argc {
		/* these options take no arguments */
        if argv[i] == "-v" || argv[i] == "--version" {
            println!("racf-VERSION"); // TODO version
            exit(0);
        } else if argv[i] == "-l" || argv[i] == "--list" { /* stats about the system */
            info();
            exit(0);
        } else if argv[i] == "-t" || argv[i] == "--enable-turbo" { /* turbo on */
            turbo(1);
            exit(0);
        } else if argv[i] == "-T" || argv[i] == "--disable-turbo" { /* turbo off */
            turbo(0);
            exit(0);
        } else if argv[i] == "-r" || argv[i] == "--run-once" { /* turbo off */
			exit(0);
		} else if i + 1 == argc {
			usage();
		/* these options take one argument */
		} else if argv[i] == "-g" || argv[i] == "--governor" { /* set governor */
			setgovernor(&argv[i + 1]);
			exit(0);
		} else {
			usage();
        }
    }

	//float threshold = (75 * cpus) / 100;
	//int charge = ischarging();
	//unsigned int tb = charge ? acturbo : batturbo;
	//setgovernor(charge ? acgovernor : batgovernor);
	//turbo(tb != Never
	//&& (tb == Always
	//|| cpuperc() >= mincpu
	//|| avgtemp() >= mintemp
	//|| avgload() >= threshold));

    /* config opts */
    let _interval = 10;
    let mincpu: f64 = 10.0;
    let mintemp: u32 = 10;


    let man = battery::Manager::new().unwrap();
    let cpus = num_cpus::get();
	let threshold: f64 = ((75 * cpus) / 100) as f64;
    let mut cpuperc = Cpu::perc(Duration::from_secs(1));

	loop {
        let btt = man.batteries().unwrap().next().unwrap();
        let charging = if btt.unwrap().state() == battery::State::Charging { true } else { false };
        let gov = if charging { "performance" } else { "powersafe" };
        let tb  = if charging { 1 } else { 0 };

        println!("{}", charging);
        setgovernor(&gov);
        if avgload() >= threshold ||
            cpuperc >= mincpu ||
            Cpu::temp() >= mintemp
        {
            turbo(tb);
        }
        cpuperc = Cpu::perc(Duration::from_secs(_interval));
    };
}

fn info() {
    println!("Turbo boost is {}",
             if Cpu::turbo() == true { "enabled" } else { "disabled" }
             );
    println!("Average temperature: {} °C", Cpu::temp());
    println!("Average cpu percentage: {:.2}%",
             Cpu::perc(std::time::Duration::from_millis(200))
             );
    let manager = battery::Manager::new().unwrap();
    for (idx, maybe_battery) in manager.batteries().unwrap().enumerate() {
        let b = maybe_battery.unwrap();
        //println!("Battery #{}:", idx);
        //println!("Vendor: {}", b.vendor().unwrap());
        //println!("Model: {}", b.model().unwrap());
        println!("Using battery #{}, state: {}", idx, b.state());
        break;
    }


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
}

fn turbo(on: i8) {
    let turbopath;
    let intelpstate = "/sys/devices/system/cpu/intel_pstate/no_turbo";
    let cpufreq = "/sys/devices/system/cpu/cpufreq/boost";

    if Path::new(intelpstate).exists() {
        turbopath = intelpstate;
    } else if Path::new(cpufreq).exists() {
        turbopath = cpufreq;
    } else { /* turbo boost is not supported */
        return;
    }

	/* change state of turbo boost */
    let mut fp = File::create(turbopath).expect("unable to create file");
    fp.write_all(on.to_string().as_bytes()).expect("Could not write");
}

fn setgovernor(gov: &str) {
    let cpus = num_cpus::get();

    for i in 0..cpus {
        let mut fp = File::create(
            format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", i)
            ).expect("unable to create file");
        fp.write_all(gov.as_bytes()).expect("Could not write");
	}
}

fn usage() {
	die!("usage: sacf [-blrtTv] [-g governor]");
}

//fn avgload() -> [f64; 3] {
fn avgload() -> f64 {
        let mut firstline = String::new();
        let mut buffer = std::io::BufReader::new(
                    File::open("/proc/loadavg").unwrap()
                    );
        buffer.read_line(&mut firstline).expect("Unable to read line");
        let mut s = firstline.split_ascii_whitespace();
        let min1  = s.next().unwrap().parse::<f64>().unwrap();
       // let min5  = s.next().unwrap().parse::<f64>().unwrap();
       // let min15 = s.next().unwrap().parse::<f64>().unwrap();

        //[ min1, min5, min15 ]
        min1
}
