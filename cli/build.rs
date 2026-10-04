//! The version `ev --version` prints. A build of the release's own tag prints the version
//! alone; any other build from the repository (the one installed for the inventory agent
//! between releases) says which commit it is, so a dev build never passes for the release.

use std::process::Command;

fn main() {
    let version = env!("CARGO_PKG_VERSION");
    let git = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.git");
    for f in ["HEAD", "refs/heads", "refs/tags", "packed-refs"] {
        println!("cargo:rerun-if-changed={}", git.join(f).display());
    }
    let describe = Command::new("git")
        .args(["describe", "--tags", "--always"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|d| !d.is_empty());
    // The release workflow builds the tag from a shallow checkout that may not see the tag
    // itself; GitHub names the tag it runs for.
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    let tag = format!("v{version}");
    let released = std::env::var("GITHUB_REF_NAME").is_ok_and(|r| r == tag);
    let shown = match describe {
        Some(d) if !released && d != tag => format!("{version} (dev {d})"),
        _ => version.to_string(),
    };
    println!("cargo:rustc-env=EV_VERSION={shown}");
}
