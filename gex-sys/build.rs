use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::fs;

extern crate bindgen;
extern crate cc;

pub fn run_command(which: &str, cmd: &mut Command) {
    let msg = format!("Failed to execute {:?}", cmd);
    let status = cmd.status().expect(&msg);
    assert!(status.success(), "{}", which);
}

fn contains_file(path: &Path, filename: &str) -> bool {
    assert!(path.is_dir());
    let mut contains = false;
    for f in path.read_dir().unwrap() {
        if f.unwrap().file_name().to_str().unwrap() == filename {
            contains = true;
        }
    }
    contains
}

const GASNET_CONDUIT: &str = "udp";
const GASNET_THREADING_ENV: &str = "par";
const GASNET_CONDUIT_LIST: [&str; 6] = ["udp", "mpi", "smp", "ucx", "ibv", "aries"];
const GASNET_WRAPPER: &str = "gasnet_wrapper";
const GASNET_LIBAMUDP: &str = "amudp";

pub fn main() {
    // TODO: cleanup everythin before install

    // config contains version info
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);
    let libdir = out_dir.join("lib");
    let gasnet_dir = fs::canonicalize("gasnet").unwrap();
    let abs_path = |s:&str| gasnet_dir.join(s);
    let working_dir = out_dir.join("gasnet");
    if !working_dir.exists(){
        fs::create_dir(&working_dir).unwrap();
    }

    // bootstrap
    let mut bt = Command::new("/bin/sh");
    bt.arg(abs_path("Bootstrap")).current_dir(&working_dir);
    if !abs_path("configure").exists() {
        // no re-bootstrap
        if !abs_path("Bootstrap").exists() {
            panic!("Can not find the Bootstrap script for GASNET. Did your forget \"git clone --recursive\"?")
        }
        run_command("bootstrap", &mut bt);
    }

    // configure
    let mut cfg = Command::new("/bin/sh");
    let envs = vec![("CFLAGS", "-fPIC"), ("CXXFLAGS", "-fPIC")];
    cfg.envs(envs);
    cfg.arg(abs_path("configure"))
        .arg(format!("--prefix={}", out_dir.display()))
        .arg("--enable-par")
        .arg("--disable-parsync")
        .arg("--disable-seq");
    #[cfg(debug_assertions)]
    cfg.arg("--enable-debug");
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
    mk.arg(GASNET_THREADING_ENV).arg("-j");
    mk.current_dir(&working_dir);
    run_command("make", &mut mk);

    // install
    let mut is = Command::new("make");
    is.arg("install");
    is.current_dir(&working_dir);
    let out_include_dir = out_dir.join("include");
    if !out_include_dir.is_dir() || !contains_file(&out_include_dir, "gasnet.h") {
        // only install once. That could cause problem. But if installed, it
        // changes some new timestamp, forcing the cargo rebuild.
        // TODO: doing that will cause compiling twice from the first build
        run_command("install", &mut is);
    }

    // choose parallel build library
    let libgasnet = format!("gasnet-{}-{}", GASNET_CONDUIT, GASNET_THREADING_ENV);
    println!("cargo:rustc-link-lib=static={}", libgasnet);
    println!("cargo:rustc-link-lib=static={}", GASNET_LIBAMUDP);
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=dylib=c++");
    } else if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    } else {
        panic!("platform unsupported")
    }
    println!("cargo:rustc-link-search=native={}", libdir.display());

    let conduit_dir = format!("include/{}-conduit", GASNET_CONDUIT);
    let headers: Vec<&str> = vec![&conduit_dir, "include"];
    let headers: Vec<PathBuf> = headers.iter().map(|dir| out_dir.join(dir)).collect();

    // compile wrapper
    let mut builder = cc::Build::new();
    builder
        .file(format!("src/{}.c", GASNET_WRAPPER))
        .include("src");
    for h in headers.iter() {
        builder.include(h);
    }
    builder.static_flag(true).compile(GASNET_WRAPPER);

    // ask rustc to link against the wrapper
    println!("cargo:rustc-link-lib=static={}", GASNET_WRAPPER);
    println!("cargo:rustc-link-search=native={}", out_dir.display());

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
}

#[cfg(test)]
mod test {
    #[test]
    pub fn test_contains_file() {
        assert_eq! {contains_file(Path::from("."), "build.rs"), true};
        assert_eq! {contains_file(Path::from("."), "nobuild.rs"), false};
    }
}
