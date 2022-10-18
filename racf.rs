use glob::glob;
use std::env;
use std::process::exit;
use std::thread::sleep;
use std::time::Duration;

/* macros */
macro_rules! die {
    ($fmt:expr) => ({ print!(concat!($fmt, "\n")); std::process::exit(-1) });
    ($fmt:expr, $($arg:tt)*) => ({ print!(concat!($fmt, "\n"), $($arg)*); std::process::exit(-1) });
}

static VERSION: u32 = 0;

fn main() {
    let argv: Vec<String> = env::args().collect();
    let argc = argv.len();
    let interval = 10;

	for i in 1..argc {
		/* these options take no arguments */
        if argv[i] == "-v" || argv[i] == "--version" {
            println!("racf-{}", VERSION);
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
        } else if argv[i] == "-b" || argv[i] == "--daemon" { /* turbo off */
			daemonize();
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
        sleep(Duration::from_secs(interval));
	};
}

fn info() {
    let cpus = 8;
	let first  = "/sys/devices/system/cpu/cpu";
	let scgov  = "/cpufreq/scaling_governor";
	let scfreq = "/cpufreq/scaling_cur_freq";
	let scdvr  = "/cpufreq/scaling_driver";
    let mut path = String::new();

	println!("Cores: {}", cpus);
	println!("AC adapter status: {}", if ischarging() { "Charging" } else { "Disconnected" });
	println!("Average system load: {}", "avgload");
	println!("System temperature: {} °C", "avgtemp");
    for i in 0..=cpus {
		/* governor */
        path = format!("{}{}{}", first, i, scgov);
        println!("{}", path);

		/* current frequency */
        path = format!("{}{}{}", first, i, scfreq);

		/* driver */
        path = format!("{}{}{}", first, i, scdvr);
    }
}

fn run() {
    println!("run()");
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

fn daemonize() {
    println!("daemonize()");
}

fn turbo(i: i32) {
    println!("turbo({})", i);
}

fn setgovernor(gov: &String) {
    println!("setgovernor(\"{}\")", gov);
}

fn usage() {
	die!("usage: sacf [-blrtTv] [-g governor]");
}

