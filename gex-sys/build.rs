use std::convert::TryFrom;
use std::convert::TryInto;
use std::env;
use std::fs;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::{Error, ErrorKind};
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

fn mpi_conduit_flags(ctx: &BuildContext) {
    println!("cargo:rustc-link-lib=static={}", "ammpi");
    mpi_probe::set_mpi_link_flags(ctx);
}

fn ibv_conduit_flags() {}

mod mpi_probe {
    use super::*;
    fn find_mpicc_path(mak: &Path) -> io::Result<PathBuf> {
        let lines = BufReader::new(fs::File::open(mak).unwrap()).lines();
        let mut mpicc_path = None;
        for line in lines {
            if let Ok(l) = line {
                let blanks = &[' ', '\t'][..];
                if l.trim_matches(blanks).starts_with("GASNET_LD_OVERRIDE") {
                    let found = l
                        .split('=')
                        .last()
                        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "missing mpicc path"))?;
                    let found = found.trim_matches(blanks);
                    mpicc_path = Some(PathBuf::from(found));
                }
            }
        }

        mpicc_path.ok_or(Error::new(
            ErrorKind::InvalidData,
            "GASNET_LD_OVERRIDE not found",
        ))
    }

    pub(super) fn set_mpi_link_flags(ctx: &BuildContext) {
        let make_fragment = ctx.conduit_include_dir.join(format!(
            "{}-{}.mak",
            ctx.conduit.as_ref(),
            GASNET_THREADING_ENV
        ));
        let mpicc_path = find_mpicc_path(make_fragment.as_path()).unwrap();
        // set libs from mpicc --show
        probe_mpicc_command(mpicc_path.as_path());
        // set from envs
        let mpi_cflags: Option<String> = env::var("MPI_CFLAGS").ok();
        let mpi_libs: Option<String> = env::var("MPI_LIBS").ok();
        let flags = |s: &str| {
            // only get flags start with -l or -L
            let f: Vec<_> = s
                .split(' ')
                .filter(|s| s.starts_with("-l") || s.starts_with("-L"))
                .collect();
            f.join(" ")
        };
        mpi_cflags.map(|s| println!("cargo:rustc-flags={}", flags(&s)));
        mpi_libs.map(|s| println!("cargo:rustc-flags={}", flags(&s)));
    }

    fn probe_mpicc_command(mpicc: &Path) {
        let lib = match rsmpi_probe::probe(mpicc) {
            Ok(lib) => lib,
            Err(errs) => {
                println!("Could not find MPI library for various reasons:\n");
                for (i, err) in errs.iter().enumerate() {
                    println!("Reason #{}:\n{}\n", i, err);
                }
                panic!();
            }
        };
        for dir in &lib.lib_paths {
            println!("cargo:rustc-link-search=native={}", dir.display());
        }
        for lib in &lib.libs {
            println!("cargo:rustc-link-lib={}", lib);
        }
    }

    mod rsmpi_probe {
        // code from https://github.com/rsmpi/rsmpi/tree/master/build-probe-mpi
        #[derive(Clone, Debug)]
        pub struct Library {
            /// Names of the native MPI libraries that need to be linked
            pub libs: Vec<String>,
            /// Search path for native MPI libraries
            pub lib_paths: Vec<PathBuf>,
            /// Search path for C header files
            pub include_paths: Vec<PathBuf>,
            /// The version of the MPI library
            pub version: String,
            _priv: (),
        }

        use super::*;
        use pkg_config::Config;
        use std::ffi::OsStr;
        use std::{self, error::Error, path::PathBuf, process::Command};

        impl From<pkg_config::Library> for Library {
            fn from(lib: pkg_config::Library) -> Self {
                Library {
                    libs: lib.libs,
                    lib_paths: lib.link_paths,
                    include_paths: lib.include_paths,
                    version: lib.version,
                    _priv: (),
                }
            }
        }

        fn probe_via_mpicc(mpicc: &OsStr) -> std::io::Result<Library> {
            // Capture the output of `mpicc -show`. This usually gives the actual compiler command line
            // invoked by the `mpicc` compiler wrapper.
            Command::new(mpicc).arg("-show").output().map(|cmd| {
                let output =
                    String::from_utf8(cmd.stdout).expect("mpicc output is not valid UTF-8");
                // Collect the libraries that an MPI C program should be linked to...
                let libs = collect_args_with_prefix(output.as_ref(), "-l");
                // ... and the library search directories...
                let libdirs = collect_args_with_prefix(output.as_ref(), "-L")
                    .into_iter()
                    .map(PathBuf::from)
                    .collect();
                // ... and the header search directories.
                let headerdirs = collect_args_with_prefix(output.as_ref(), "-I")
                    .into_iter()
                    .map(PathBuf::from)
                    .collect();

                Library {
                    libs,
                    lib_paths: libdirs,
                    include_paths: headerdirs,
                    version: String::from("unknown"),
                    _priv: (),
                }
            })
        }

        /// splits a command line by space and collects all arguments that start with `prefix`
        fn collect_args_with_prefix(cmd: &str, prefix: &str) -> Vec<String> {
            cmd.split_whitespace()
                .filter_map(|arg| {
                    if arg.starts_with(prefix) {
                        Some(arg[2..].to_owned())
                    } else {
                        None
                    }
                })
                .collect()
        }

        /// Probe the environment for an installed MPI library
        pub fn probe(mpicc: &Path) -> Result<Library, Vec<Box<dyn Error>>> {
            let mut errs = vec![];

            match probe_via_mpicc(mpicc.as_ref()) {
                Ok(lib) => return Ok(lib),
                Err(err) => {
                    let err: Box<dyn Error> = Box::new(err);
                    errs.push(err)
                }
            }

            match Config::new().cargo_metadata(false).probe("mpich") {
                Ok(lib) => return Ok(Library::from(lib)),
                Err(err) => {
                    let err: Box<dyn Error> = Box::new(err);
                    errs.push(err)
                }
            }

            match Config::new().cargo_metadata(false).probe("openmpi") {
                Ok(lib) => return Ok(Library::from(lib)),
                Err(err) => {
                    let err: Box<dyn Error> = Box::new(err);
                    errs.push(err)
                }
            }

            Err(errs)
        }
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

    let ctx = BuildContext::new(conduit_enabled[0]); // use the first

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

    let append_envs = |cmd: &mut Command, key: &str, value: &str| {
        let update = match env::var(key) {
            Ok(v) => v + " " + value,
            Err(_) => value.to_owned(),
        };
        cmd.env(key, update);
    };

    // configure
    let mut cfg = Command::new("/bin/sh");
    // rust enables PIC by default on linux
    append_envs(&mut cfg, "CFLAGS", "-fPIE");
    append_envs(&mut cfg, "CXXFLAGS", "-fPIE");
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
    mk.arg("AM_CFLAGS=-fPIE").arg("AM_CXXFLAGS=-fPIE");
    mk.arg(GASNET_THREADING_ENV).arg("-j");
    mk.current_dir(&ctx.working_dir);
    run_command("make", &mut mk);

    // install
    let mut is = Command::new("make");
    is.arg("install");
    is.current_dir(&ctx.working_dir);
    run_command("install", &mut is);

    // common flags for crayfish linking
    let libgasnet = format!("gasnet-{}-{}", ctx.conduit.as_ref(), GASNET_THREADING_ENV);
    println!("cargo:rustc-link-lib=static={}", libgasnet);
    println!("cargo:rustc-link-lib=dylib=m"); // specified by udp-conduit/conduit.mak
    println!(
        "cargo:rustc-link-search=native={}",
        ctx.gasnet_lib_dir.display()
    );

    // conduit flags
    match ctx.conduit {
        GasnetConduit::Udp => udp_conduit_flags(),
        GasnetConduit::Mpi => mpi_conduit_flags(&ctx),
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
