extern crate once_cell;
extern crate sys_info;
use crate::logging::*;
use once_cell::sync::Lazy;
use std::env;
use std::time;

pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");

fn env_name_with_prefix(name: &str) -> String {
    let mut pkg_name_upper = String::from(PKG_NAME);
    pkg_name_upper.make_ascii_uppercase();
    let pkg_name_upper = pkg_name_upper.replace("-", "_");
    format!("{}_{}", pkg_name_upper, name)
}

pub fn env_with_prefix(name: &str) -> Result<String, env::VarError> {
    let name = env_name_with_prefix(name);
    env::var(&name)
}

pub static HOSTNAME: Lazy<String> = Lazy::new(|| match sys_info::hostname() {
    Ok(hostname) => hostname,
    Err(e) => {
        warn!("Get host name error:{}", e);
        String::from("Unknown")
    }
});

pub static NUM_CPUS: Lazy<usize> = Lazy::new(|| {
    #[cfg(not(test))]
    let default_num = 1;
    #[cfg(test)] // test requires the cpu num > 1
    let default_num = 2;
    let warn_dft = || warn!("use default worker number: {}", default_num);
    match env_with_prefix("NUM_CPUS") {
        Ok(s) => {
            let n: usize = match s.parse() {
                Ok(n) => n,
                Err(_) => {
                    warn!("bad worker number: {}", s);
                    warn_dft();
                    default_num
                }
            };
            if !(0 < n && n <= default_num) {
                warn!(
                    "worker number must be in range 1~{}, given: {}",
                    default_num, n
                );
                warn_dft();
                default_num
            } else {
                n
            }
        }
        Err(_) => default_num,
    }
});
pub static MAX_BUFFER_LIFETIME: Lazy<time::Duration> = Lazy::new(|| {
    let default = time::Duration::from_millis(1);
    let parse_dur = |s: &str| -> Result<time::Duration, ()> {
        if s.len() > 2 {
            let count: u64 = s[..s.len() - 2].parse().map_err(|_| ())?;
            if s.ends_with("ms") {
                Ok(time::Duration::from_millis(count))
            } else if s.ends_with("us") {
                Ok(time::Duration::from_micros(count))
            } else if s.ends_with("ns") {
                Ok(time::Duration::from_nanos(count))
            } else {
                Err(())
            }
        } else {
            Err(())
        }
    };
    match env_with_prefix("MAX_SEND_INTERVAL") {
        Ok(s) => match parse_dur(&s) {
            Ok(dur) => dur,
            Err(_) => {
                warn!(
                    "bad duration: {}. duration should be [0-9]+(ms|us|ns), use default {:?}",
                    s, default
                );
                default
            }
        },
        Err(_) => default,
    }
});

pub fn show_data() {
    let show = vec![
        format!("NUM_CPUS: {}", *NUM_CPUS),
        format!("MAX_SEND_INTERVAL: {:?}", *MAX_BUFFER_LIFETIME),
    ];
    debug!("run Crayfish with:\n{}", show.join("\n"));
}
