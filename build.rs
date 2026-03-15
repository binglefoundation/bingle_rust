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
    let build_num_file = ".build_number";
    let mut build_num: u32 = fs::read_to_string(build_num_file)
        .unwrap_or_else(|_| "0".to_string())
        .trim()
        .parse()
        .unwrap_or(0);

    // Only increment if we are building for the first time or if something changed.
    // build.rs is only run by cargo if rerun-if-changed inputs have changed.
    
    build_num += 1;
    fs::write(build_num_file, build_num.to_string())?;

    println!("cargo:rustc-env=BINGLE_BUILD_NUMBER={}", build_num);
    
    // Rerun if any source file changes
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=.git/HEAD"); 
    
    // DO NOT add .build_number to rerun-if-changed to avoid circular triggers
    // The build script itself increments it, and that's fine.
    // If the user wants to force a rebuild, they should touch src/lib.rs or Cargo.toml.

    Ok(())
}
