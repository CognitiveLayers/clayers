use std::process::Command;

fn main() {
    // Re-run if HEAD changes (new commit, tag, checkout)
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/HEAD");

    let version = Command::new("git")
        .args(["describe", "--tags", "--always"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
            // Strip leading 'v' prefix (v0.1.3 -> 0.1.3)
            let v = v.strip_prefix('v').unwrap_or(&v).to_string();
            if v.is_empty() { None } else { Some(v) }
        })
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    println!("cargo:rustc-env=CLAYERS_VERSION={version}");
}
