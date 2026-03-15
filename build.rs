use std::fs;
use vergen::EmitBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Standard vergen info
    EmitBuilder::builder()
        .all_build()
        .all_cargo()
        .all_git()
        .emit()?;

    // Build number logic
    let mut build_num: u32 = fs::read_to_string(".build_number")
        .unwrap_or_else(|_| "0".to_string())
        .trim()
        .parse()
        .unwrap_or(0);

    build_num += 1;
    fs::write(".build_number", build_num.to_string())?;

    println!("cargo:rustc-env=BINGLE_BUILD_NUMBER={}", build_num);
    
    // Ensure we don't re-run just because .build_number changed
    // We want to re-run if source changes, which will then increment the build number.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    Ok(())
}
