#[test]
fn invalid_governor() {
    use crate::MainE;
    use crate::Config;

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

    let Err(MainE::WrongGov(_e)) = file.validate() else {
        eprintln!("Parsed an invalid governor.");
        return;
    };
}

#[test]
fn invalid_turbo() {
    use crate::MainE;
    use crate::Config;

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
    if f.is_ok() { // Should error out
        dbg!(&f);
        panic!("Parsed an invalid governor.\n'{:?}'", f);
    };
}
