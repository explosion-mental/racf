use std::env;
use std::process::exit;

static VERSION: u32 = 0;
static CPUS: u32 = 8;

fn main() {
    let argv: Vec<String> = env::args().collect();
    let argc = argv.len();

    if argv.len() == 1 {
        println!("No arguments");
        exit(1);
    }

	for mut i in 1..argc {
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
}

fn info() {
	println!("Cores: {}", CPUS);
	println!("AC adapter status: {}", "ischarging");
	println!("Average system load: {}", "avgload");
	println!("System temperature: {} °C", "avgtemp");
}

fn run() {
    println!("run()");
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
    println!("usage()");
    exit(0);
}

