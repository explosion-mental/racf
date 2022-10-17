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

/* enums */
enum TurboPathIdx { INTEL, CPUFREQ, BROKEN }
enum TurboAction { Always, Never, Auto }

const TurboPath: [&str; 3] = [
	"/sys/devices/system/cpu/intel_pstate/no_turbo",
	"/sys/devices/system/cpu/cpufreq/boost",
	"", /* no turbo boost support */
];

static VERSION: u32 = 0;
static CPUS: u32 = 8;
static interval: u64 = 10;

fn main() {
    let argv: Vec<String> = env::args().collect();
    let argc = argv.len();

	for mut i in 1..argc {
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
            //no ++ or -- huh?
            i += 1;
			setgovernor(&argv[i]);
            i -= 1;
			exit(0);
		} else {
			usage();
        }

    }

	//while true {
		run();
        //sleep(Duration::from_secs(interval));
	//}
}

fn info() {
	println!("Cores: {}", CPUS);
	println!("AC adapter status: {}", ischarging());
	println!("Average system load: {}", "avgload");
	println!("System temperature: {} °C", "avgtemp");
}

fn run() {
    println!("run()");
}

fn ischarging() -> bool {
    for entry in glob("/sys/class/power_supply/A*/online").expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => return true,

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
    println!("turbo()");
}

fn setgovernor(gov: &String) {
    println!("setgovernor(\"{}\")", gov);
}

fn usage() {
	die!("usage: sacf [-blrtTv] [-g governor]");
}

