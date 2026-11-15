// SPDX-License-Identifier: Apache-2.0
//! Print a design's physical hierarchy — or its design-data report — in upstream's own format,
//! so the two can be diffed. The driver is [`vyges_mpl::driver`]; `vyges-mpl run` drives the same
//! stages and applies the commit this prints.

/// The binary allocates through mimalloc, as the reference allocates through the tcmalloc its
/// Bazel build links (`malloc = @tcmalloc`); see Cargo.toml for the pin.
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout().lock();
    // ⚠️ With the partitioner linked, the stage dumps reach the designs that need it — a design
    // with a large flat cluster is otherwise REFUSED here while `run` places it, and every stage
    // scorer reports it as "other" instead of scoring it.
    #[cfg(feature = "par")]
    let result = vyges_mpl::driver::run_with(&args, &mut out, Some(&mut vyges_mpl::par::Par));
    #[cfg(not(feature = "par"))]
    let result = vyges_mpl::driver::run(&args, &mut out);
    if let Err(e) = result {
        eprintln!("{}", e.message);
        std::process::exit(i32::from(e.code));
    }
}
