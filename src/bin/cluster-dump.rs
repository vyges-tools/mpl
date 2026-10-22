// SPDX-License-Identifier: Apache-2.0
//! Print a design's physical hierarchy — or its design-data report — in upstream's own format,
//! so the two can be diffed. The driver is [`vyges_mpl::driver`]; `vyges-mpl run` drives the same
//! stages and applies the commit this prints.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout().lock();
    if let Err(e) = vyges_mpl::driver::run(&args, &mut out) {
        eprintln!("{}", e.message);
        std::process::exit(i32::from(e.code));
    }
}
