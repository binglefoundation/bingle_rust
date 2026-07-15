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
    let build_num_file = "../.build_number";
    let mut build_num: u32 = fs::read_to_string(build_num_file)
        .unwrap_or_else(|_| "0".to_string())
        .trim()
        .parse()
        .unwrap_or(0);

    if should_bump_build_number() {
        build_num += 1;
        fs::write(build_num_file, build_num.to_string())?;
    }

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

fn should_bump_build_number() -> bool {
    let get_output = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    };

    if get_output(&["rev-parse", "--abbrev-ref", "HEAD"]).as_deref() != Some("master") {
        return false;
    }

    if let Some(toplevel_path) = get_output(&["rev-parse", "--show-toplevel"]) {
        let path = std::path::Path::new(&toplevel_path);
        if path.file_name().and_then(|s| s.to_str()) != Some("master") {
            return false;
        }
        // Require a git checkout at the toplevel, but accept both forms of `.git`: a directory in
        // a primary checkout, or a file (a gitdir pointer) in a linked worktree. Using is_dir()
        // here wrongly excluded worktrees, so builds run from a worktree never bumped the number.
        if !path.join(".git").exists() {
            return false;
        }
    } else {
        return false;
    }

    true
}
