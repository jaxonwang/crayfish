use std::convert::TryFrom;
use std::convert::TryInto;
use std::env;
use std::fs;
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

const GASNET_THREADING_ENV: &str = "seq";
const GASNET_CONDUIT_LIST: [&str; 6] = ["ibv", "mpi", "udp", "smp", "ucx", "aries"];
const GASNET_WRAPPER: &str = "gasnet_wrapper";
const GASNET_LIBAMUDP: &str = "amudp";
const REBUILD_IF_ENVS_CHANGE: [&str; 17] = [
    // mpi flags
    "MPI_CC",
    "MPI_CFLAGS",
    "MPI_LIBS",
    // ibv flags
    "IBV_HOME",
    "IBV_CFLAGS",
    "IBV_LIBS",
    "IBV_LDFLAGS",
    "FH_CFLAGS",
    // common C/CXX flags
    "CC",
    "CFLAGS",
    "LDFLAGS",
    "LIBS",
    "CPPFLAGS",
    "CPP",
    "CXX",
    "CXXFLAGS",
    "CXXCPP",
];

#[allow(dead_code)]
#[derive(Ord, PartialOrd, PartialEq, Eq, Copy, Clone)]
enum GasnetConduit {
    Ibv = 0,
    Mpi = 1,
    Udp = 2,
}

impl AsRef<str> for GasnetConduit {
    fn as_ref(&self) -> &str {
        GASNET_CONDUIT_LIST[*self as usize]
    }
}

impl TryFrom<usize> for GasnetConduit {
    type Error = ();

    fn try_from(v: usize) -> Result<Self, Self::Error> {
        match v {
            x if x == GasnetConduit::Ibv as usize => Ok(GasnetConduit::Ibv),
            x if x == GasnetConduit::Mpi as usize => Ok(GasnetConduit::Mpi),
            x if x == GasnetConduit::Udp as usize => Ok(GasnetConduit::Udp),
            _ => Err(()),
        }
    }
}

fn udp_conduit_flags() {
    println!("cargo:rustc-link-lib=static={}", GASNET_LIBAMUDP);
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=dylib=c++");
    } else if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    } else {
        panic!("platform unsupported")
    }
}

fn mpi_conduit_flags() {
    println!("cargo:rustc-link-lib=static={}", "ammpi");
}

fn ibv_conduit_flags() {}

mod mpi_probe {
    use super::*;
    fn mpicc_path_from_gasnet(ctx: BuildContext) {
        // let conduit_header_dir = conduit_header_dir
    }
}

struct BuildContext {
    out_dir: PathBuf,
    working_dir: PathBuf,         // buildpath/out/gasnet
    gasnet_src_dir: PathBuf,      // .src/gasnet
    gasnet_lib_dir: PathBuf,      // buildpath/out/lib
    gasnet_include_dir: PathBuf,  // buildpath/out/include
    conduit_include_dir: PathBuf, // buildpath/out/include/udp-conduit
    conduit: GasnetConduit,
}

impl BuildContext {
    fn new(conduit: GasnetConduit) -> Self {
        let out_dir = env::var("OUT_DIR").unwrap();
        let out_dir = fs::canonicalize(&out_dir).unwrap();
        let gasnet_lib_dir = out_dir.join("lib");
        let gasnet_include_dir = out_dir.join("include");
        let conduit_include_dir = gasnet_include_dir.join(format!("{}-conduit", conduit.as_ref()));
        let gasnet_src_dir = fs::canonicalize("gasnet").unwrap();
        let working_dir = out_dir.join("gasnet");
        // create working directory
        if !working_dir.exists() {
            fs::create_dir(&working_dir).unwrap();
        }
        BuildContext {
            out_dir,
            working_dir,
            gasnet_lib_dir,
            gasnet_include_dir,
            gasnet_src_dir,
            conduit_include_dir,
            conduit,
        }
    }

    fn build_script_path(&self, script_name: impl AsRef<Path>) -> PathBuf {
        self.gasnet_src_dir.join(script_name)
    }
}

pub fn main() {
    // TODO: cleanup everythin before install

    let mut conduit_enabled = vec![];
    #[cfg(feature = "udp")]
    conduit_enabled.push(GasnetConduit::Udp);
    #[cfg(feature = "mpi")]
    conduit_enabled.push(GasnetConduit::Mpi);
    #[cfg(feature = "ibv")]
    conduit_enabled.push(GasnetConduit::Ibv);
    conduit_enabled.sort(); // order by precedence

    let conduit_to_use = conduit_enabled[0];
    let ctx = BuildContext::new(conduit_to_use);

    // config contains version info
    println!("cargo:rerun-if-changed=src/gasnet_wrapper.h");
    println!("cargo:rerun-if-changed=src/gasnet_wrapper.c");
    println!("cargo:rerun-if-changed=gasnet");
    for env in REBUILD_IF_ENVS_CHANGE.iter() {
        println!("cargo:rerun-if-env-changed={}", env);
    }

    // bootstrap
    let mut bt = Command::new("/bin/sh");
    bt.arg(ctx.build_script_path("Bootstrap"))
        .current_dir(&ctx.working_dir);
    if !ctx.build_script_path("configure").exists() {
        // no re-bootstrap
        if !ctx.build_script_path("Bootstrap").exists() {
            panic!("Can not find the Bootstrap script for GASNET. Did your forget \"git clone --recursive\"?")
        }
        run_command("bootstrap", &mut bt);
    }

    // configure
    let mut cfg = Command::new("/bin/sh");
    let envs = vec![("CFLAGS", "-fPIC"), ("CXXFLAGS", "-fPIC")];
    cfg.envs(envs);
    cfg.arg(ctx.build_script_path("configure"))
        .arg(format!("--prefix={}", &ctx.out_dir.display()))
        .arg("--enable-seq")
        .arg("--disable-parsync")
        .arg("--disable-par");
    #[cfg(debug_assertions)]
    cfg.arg("--enable-debug");
    for (i, c) in GASNET_CONDUIT_LIST.iter().enumerate() {
        match i.try_into() {
            Ok(conduit) => {
                if conduit_enabled.contains(&conduit) {
                    cfg.arg(format!("--enable-{}", c));
                } else {
                    cfg.arg(format!("--disable-{}", c));
                }
            }
            Err(_) => {
                cfg.arg(format!("--disable-{}", c));
            }
        }
    }
    cfg.current_dir(&ctx.working_dir);
    run_command("configure", &mut cfg);

    // make
    let mut mk = Command::new("make");
    mk.arg(GASNET_THREADING_ENV).arg("-j");
    mk.current_dir(&ctx.working_dir);
    run_command("make", &mut mk);

    // install
    let mut is = Command::new("make");
    is.arg("install");
    is.current_dir(&ctx.working_dir);
    run_command("install", &mut is);

    // common flags
    let libgasnet = format!(
        "gasnet-{}-{}",
        conduit_to_use.as_ref(),
        GASNET_THREADING_ENV
    );
    println!("cargo:rustc-link-lib=static={}", libgasnet);
    println!("cargo:rustc-link-lib=dylib=m"); // specified by udp-conduit/conduit.mak
    println!(
        "cargo:rustc-link-search=native={}",
        ctx.gasnet_lib_dir.display()
    );

    // conduit flags
    match conduit_to_use {
        GasnetConduit::Udp => udp_conduit_flags(),
        GasnetConduit::Mpi => mpi_conduit_flags(),
        GasnetConduit::Ibv => ibv_conduit_flags(),
    }

    // compile wrapper
    let mut builder = cc::Build::new();
    builder
        .file(format!("src/{}.c", GASNET_WRAPPER))
        .include("src");
    let headers = [&ctx.gasnet_include_dir, &ctx.conduit_include_dir];
    for h in headers.iter() {
        builder.include(h);
    }
    builder.static_flag(true).compile(GASNET_WRAPPER);

    // ask rustc to link against the wrapper
    println!("cargo:rustc-link-lib=static={}", GASNET_WRAPPER);
    println!("cargo:rustc-link-search=native={}", ctx.out_dir.display());

    // bindgen
    let mut clang_args = vec![];
    for h in headers.iter() {
        clang_args.push("-I");
        clang_args.push(h.to_str().unwrap());
    }
    let bindings = bindgen::Builder::default()
        .header(format!("src/{}.h", GASNET_WRAPPER))
        .clang_args(clang_args.iter())
        .allowlist_function("gex_.*")
        .allowlist_function("gasnet_.*")
        .allowlist_var("GEX_.*")
        .allowlist_var("GASNET_.*")
        .generate()
        .expect("Unable to generate bindings");
    bindings
        .write_to_file(ctx.out_dir.join("bindings.rs"))
        .unwrap();
}

#[cfg(test)]
mod test {
    #[test]
    pub fn test_contains_file() {
        assert_eq! {contains_file(Path::from("."), "build.rs"), true};
        assert_eq! {contains_file(Path::from("."), "nobuild.rs"), false};
    }
}
