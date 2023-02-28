use crate::{Config, MainE};

#[test]
fn invalid_governor() {
    let file: Config = toml::from_str(
"
[ac]
turbo = \"auto\"
mincpu = 30.0
mintemp = 70
interval = 60
governor = \"performance\"
[battery]
turbo = \"auto\"
mincpu = 30.0
mintemp = 70
interval = 60
governor = \"erformance\" ") // <-- should be "performance"
        .expect("NeverFailing");

    let f = file.validate();
    if f.is_ok() { // Should error out when parsing `erformance`
        dbg!(&f);
        panic!("Parsed an invalid governor.\n'{:?}'", f);
    };
}

#[test]
fn invalid_turbo() {
    let file: Config = toml::from_str(
// turbo values should be 'auto' - 'always' - 'never'
"
[ac]
turbo = \"aut\"
mincpu = 30.0
mintemp = 70
interval = 60
governor = \"performance\"
[battery]
turbo = \"auto\"
mincpu = 30.0
mintemp = 70
interval = 60
governor = \"performance\" ")
        .expect("NeverFailing");

    let f = file.validate();
    if f.is_ok() { // Should error out when parsing `aut`
        dbg!(&f);
        panic!("Parsed an invalid turbo value.\n'{:?}'", f);
    };
}
