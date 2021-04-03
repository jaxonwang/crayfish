use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

extern crate bindgen;
extern crate cc;

pub fn run_command(which: &str, cmd: &mut Command) {
    let msg = format!("Failed to execute {:?}", cmd);
    let status = cmd.status().expect(&msg);
    assert!(status.success(), "{}", which);
}

const GASNET_CONDUIT: &str = "udp";
const GASNET_THREADING_ENV: &str = "par";

const GASNET_CONDUIT_LIST: [&str; 6] = ["udp", "mpi", "smp", "ucx", "ibv", "aries"];

const GASNET_WRAPPER: &str = "gasnet_wrapper";

pub fn main() {
    // config contains version info
    println!("cargo:rerun-if-changed=gasnet/configure.in");

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);
    let libdir = out_dir.join("lib");
    let working_dir = Path::new("gasnet");

    // bootstrap
    let mut bt = Command::new("sh");
    bt.arg("Bootstrap").arg("-T").current_dir(&working_dir);
    bt.status().unwrap(); // doesn't check error fo rerun

    // configure
    let mut cfg = Command::new("sh");
    cfg.arg("configure")
        .arg(format!("--prefix={}", out_dir.display()));
    for c in GASNET_CONDUIT_LIST.iter() {
        if &GASNET_CONDUIT == c {
            cfg.arg(format!("--enable-{}", c));
        } else {
            cfg.arg(format!("--disable-{}", c));
        }
    }
    cfg.current_dir(&working_dir);
    run_command("configure", &mut cfg);

    // make
    let mut mk = Command::new("make");
    mk.arg(GASNET_THREADING_ENV);
    mk.current_dir(&working_dir);
    run_command("make", &mut mk);

    // install
    let mut is = Command::new("make");
    is.arg("install");
    is.current_dir(&working_dir);
    run_command("install", &mut is);

    // choose parallel build
    let libname = format!("gasnet-{}-{}", GASNET_CONDUIT, GASNET_THREADING_ENV);
    println!("cargo:rustc-link-lib=static={}", libname);
    println!("cargo:rustc-link-search={}", libdir.display());

    let headers: [&str; 2] = [&format!("include/{}-conduit", GASNET_CONDUIT), "include"];
    let headers: Vec<PathBuf> = headers.iter().map(|dir| out_dir.join(dir)).collect();

    // bindgen
    let mut clang_args = vec![];
    for h in headers.iter() {
        clang_args.push("-I");
        clang_args.push(h.to_str().unwrap());
    }
    let bindings = bindgen::Builder::default()
        .header(format!("src/{}.h", GASNET_WRAPPER))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .clang_args(clang_args.iter())
        .generate()
        .expect("Unable to generate bindings");
    bindings.write_to_file(out_dir.join("bindings.rs")).unwrap();

    // compile wrapper
    let mut builder = cc::Build::new();
    builder
        .file(format!("src/{}.c", GASNET_WRAPPER))
        .include("src");
    for h in headers.iter() {
        builder.include(h);
    }
    builder.static_flag(true).compile(GASNET_WRAPPER);

    // ask rustc to link the wrapper
    println!("cargo:rustc-link-lib=static={}", GASNET_WRAPPER);
    println!("cargo:rustc-link-search={}", out_dir.display());
}
