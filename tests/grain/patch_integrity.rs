//! The fork must stay reviewable.
//!
//! rhaigrain builds against a vendored rhai rather than the published crate, so
//! that the VM can reach a handful of internals rhai does not export. That is
//! only defensible while the delta is small, visibility-only, and visible in
//! one file. `vendor/rhai` is generated and gitignored; `vendor/rhai-1.25.1.patch`
//! is the checked-in artifact, and this test is what keeps them honest.
//!
//! The real work is in `scripts/check-patch.sh`, which verifies two separate
//! claims: that the patch equals the true pristine -> vendored delta, and that
//! applying it to a pristine tree reproduces what we compile against.

use std::process::Command;

#[test]
fn vendored_rhai_matches_its_patch() {
    let root = env!("CARGO_MANIFEST_DIR");

    let output = Command::new("bash")
        .arg(format!("{root}/scripts/check-patch.sh"))
        .output()
        .expect("failed to run scripts/check-patch.sh");

    if !output.status.success() {
        panic!(
            "vendored rhai does not match its patch\n\
             --- stdout ---\n{}\n--- stderr ---\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}
