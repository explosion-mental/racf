use glob::glob;
use std::env;
use std::process::exit;
use std::thread::sleep;
use std::time::Duration;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

/* macros */
macro_rules! die {
    ($fmt:expr) => ({ print!(concat!($fmt, "\n")); std::process::exit(-1) });
    ($fmt:expr, $($arg:tt)*) => ({ print!(concat!($fmt, "\n"), $($arg)*); std::process::exit(-1) });
}

fn main() {
    let argv: Vec<String> = env::args().collect();
    let argc = argv.len();
    let _interval = 10;

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
			run();
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

    die!("end of main()");

	loop {
		run();
        sleep(Duration::from_secs(_interval));
	};
}

fn info() {
    let cpus   = 8; /* TODO nproc() */
	let first  = "/sys/devices/system/cpu/cpu";
	let scgov  = "/cpufreq/scaling_governor";
	let scfreq = "/cpufreq/scaling_cur_freq";
	let scdvr  = "/cpufreq/scaling_driver";

	println!("Cores: {}", cpus);
	println!("AC adapter status: {}", if ischarging() { "Charging" } else { "Disconnected" });
	println!("Average system load: {}", "avgload");
	println!("System temperature: {} °C", "avgtemp");
    for i in 0..cpus {
        let (mut governor, mut driver, mut freq) = (String::new(), String::new(), String::new());
		/* governor */
        File::open(format!("{}{}{}", first, i, scgov))
            .expect("Cannot open file.")
            .read_to_string(&mut governor)
            .expect("Cannot read file.");

		/* current frequency */
        File::open(format!("{}{}{}", first, i, scfreq))
            .expect("Cannot open file.")
            .read_to_string(&mut freq)
            .expect("Cannot read file.");

		/* driver */
        File::open(format!("{}{}{}", first, i, scdvr))
            .expect("Cannot open file.")
            .read_to_string(&mut driver)
            .expect("Cannot read file.");

		println!("CPU{}\t{}\t{}\t{}", i, governor.trim_end(), driver.trim_end(), freq.trim_end());
    }
}

fn run() {
    let charging = ischarging();
    let gov = if charging { "performance" } else { "powersafe" };
    let tb  = if charging { 1 } else { 0 };
    let cpus = 8;
	let _threshold = (75 * cpus) / 100;

	setgovernor(&gov.to_string());
    turbo(tb);

}

fn ischarging() -> bool {
    for entry in glob("/sys/class/power_supply/A*/online").expect("Failed to read glob pattern") {
        match entry {
            Ok(_path) => return true,

            // if the path matched but was unreadable,
            // thereby preventing its contents from matching
            Err(e) => println!("{:?}", e),
        }
    }
    false
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

fn setgovernor(gov: &String) {
    let cpus   = 8; /* TODO nproc() */

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

