static CPUS: u32 = 8;

fn main() {
    for i in 1..6 {
        println!("{}", i);
    }
    info();
}

fn info() {

	println!("Cores: {}", CPUS);
	println!("AC adapter status: {}", "ischarging");
	println!("Average system load: {}", "avgload");
	println!("System temperature: {} °C", "avgtemp");
}
