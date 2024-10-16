use std::error::Error;
use std::fmt;
use std::io::{self, Write};

use base64::prelude::*;
use clap::{Arg, ArgAction, Command};
use rayon::prelude::*;
use wireguard_vanity_lib::{measure_rate, search_for_prefix};
use x25519_dalek::{PublicKey, StaticSecret};

fn format_time(t: f64) -> String {
    if t > 3600.0 {
        format!("{:.2} hours", t / 3600.0)
    } else if t > 60.0 {
        format!("{:.1} minutes", t / 60.0)
    } else if t > 1.0 {
        format!("{:.1} seconds", t)
    } else if t > 1e-3 {
        format!("{:.1} ms", t * 1e3)
    } else if t > 1e-6 {
        format!("{:.1} us", t * 1e6)
    } else if t > 1e-9 {
        format!("{:.1} ns", t * 1e9)
    } else {
        format!("{:.3} ps", t * 1e12)
    }
}

fn format_rate(rate: f64) -> String {
    if rate > 1e9 {
        format!("{:.2}e9 keys/s", rate / 1e9)
    } else if rate > 1e6 {
        format!("{:.2}e6 keys/s", rate / 1e6)
    } else if rate > 1e3 {
        format!("{:.2}e3 keys/s", rate / 1e3)
    } else if rate > 1e0 {
        format!("{:.2} keys/s", rate)
    } else if rate > 1e-3 {
        format!("{:.2}e-3 keys/s", rate * 1e3)
    } else if rate > 1e-6 {
        format!("{:.2}e-6 keys/s", rate * 1e6)
    } else if rate > 1e-9 {
        format!("{:.2}e-9 keys/s", rate * 1e9)
    } else {
        format!("{:.3}e-12 keys/s", rate * 1e12)
    }
}

fn print(res: (StaticSecret, PublicKey)) -> Result<(), io::Error> {
    let private: StaticSecret = res.0;
    let public: PublicKey = res.1;
    let private_b64 = BASE64_STANDARD.encode(private.to_bytes());
    let public_b64 = BASE64_STANDARD.encode(public.as_bytes());
    writeln!(
        io::stdout(),
        "private {}  public {}",
        &private_b64,
        &public_b64
    )
}

#[derive(Debug)]
struct ParseError(String);
impl Error for ParseError {}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let matches = Command::new("wireguard-vanity-address")
        .arg_required_else_help(true)
        .version("0.5.0")
        .author("Brian Warner <warner@lothar.com>")
        .about("finds Wireguard keypairs with a given string prefix")
        .arg(
            Arg::new("CASE")
                .short('c')
                .long("case-sensitive")
                .action(ArgAction::SetTrue)
                .help("Use case-sensitive matching"),
        )
        .arg(
            Arg::new("RANGE")
                .short('i')
                .long("in")
                .action(ArgAction::Set)
                .default_value("10")
                .help("NAME must be found within first RANGE chars of pubkey (default: 10, 0 means actual prefix)"),
        )
        .arg(
            Arg::new("NAME")
                .required(true)
                .help("string to find near the start of the pubkey"),
        )
        .get_matches();
    let case_sensitive = matches.get_flag("CASE");
    let prefix = matches.get_one::<String>("NAME").unwrap();
    let len = prefix.len();
    let end: usize = 44.min(match matches.get_one::<String>("RANGE") {
        Some(val) => val.parse().unwrap(),
        None => 0,
    });
    let end = if end == 0 { len } else { end };
    if end < len {
        return Err(ParseError(format!("range {} is too short for len={}", end, len)).into());
    }

    let offsets: u128 = 44.min((1 + end - len) as u128);
    // todo: this is an approximation, offsets=2 != double the chances
    let mut num = offsets;
    let mut denom = 1u128;
    prefix.chars().for_each(|c| {
        if !case_sensitive && c.is_ascii_alphabetic() {
            num *= 2; // letters can match both uppercase and lowercase
        }
        denom *= 64; // base64
    });
    let trials_per_key = denom / num;

    println!(
        "searching for '{}' in pubkey[0..{}], one of every {} keys should match",
        &prefix, end, trials_per_key
    );

    // get_physical() appears to be more accurate: hyperthreading doesn't
    // help us much

    if trials_per_key < 2u128.pow(32) {
        let raw_rate = measure_rate();
        println!(
            "one core runs at {}, CPU cores available: {}",
            format_rate(raw_rate),
            num_cpus::get_physical(),
        );
        let total_rate = raw_rate * (num_cpus::get_physical() as f64) / (trials_per_key as f64);
        let seconds_per_key = 1.0 / total_rate;
        println!(
            "est yield: {} per key, {}",
            format_time(seconds_per_key),
            format_rate(total_rate)
        );
    }

    println!("hit Ctrl-C to stop");

    // 1M trials takes about 10s on my laptop, so let it run for 1000s
    (0..100_000_000)
        .into_par_iter()
        .map(|_| search_for_prefix(prefix, 0, end, case_sensitive))
        .try_for_each(print)?;
    Ok(())
}
