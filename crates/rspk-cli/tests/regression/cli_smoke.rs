//! Regression: CLI smoke tests.
//!
//! Verifies that the `pk` binary starts, parses arguments, and
//! handles basic commands without panicking. All commands run in
//! `--dry-run` mode so nothing is actually installed.

use std::process::Command;

fn pk() -> Command
{
    Command::new(env!("CARGO_BIN_EXE_pk"))
}

fn assert_success(cmd: &mut Command, label: &str)
{
    let output = cmd.output().expect("failed to execute pk");
    assert!(
        output.status.success(),
        "{label} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn assert_failure(cmd: &mut Command, label: &str)
{
    let output = cmd.output().expect("failed to execute pk");
    assert!(
        !output.status.success(),
        "{label} should have failed but exited 0:\nstdout: {}",
        String::from_utf8_lossy(&output.stdout),
    );
}

// ── Basic invocation ────────────────────────────────────────────

#[test]
fn version_exits_zero()
{
    assert_success(pk().arg("--version"), "pk --version");
}

#[test]
fn help_exits_zero()
{
    assert_success(pk().arg("--help"), "pk --help");
}

#[test]
fn unknown_subcommand_exits_nonzero()
{
    assert_failure(pk().arg("frobnicate"), "pk frobnicate");
}

// ── Dry-run commands ────────────────────────────────────────────

#[test]
fn inventory_dry_run()
{
    assert_success(
        pk().arg("--dry-run").arg("inventory"),
        "pk --dry-run inventory",
    );
}

#[test]
fn install_dry_run()
{
    assert_success(
        pk().args(["--dry-run", "--quiet", "install", "ripgrep"]),
        "pk --dry-run --quiet install ripgrep",
    );
}

#[test]
fn search_dry_run()
{
    assert_success(
        pk().args(["--dry-run", "search", "curl"]),
        "pk --dry-run search curl",
    );
}

#[test]
fn resolve_dry_run()
{
    assert_success(
        pk().args(["--dry-run", "resolve", "curl"]),
        "pk --dry-run resolve curl",
    );
}

#[test]
fn installed_dry_run()
{
    assert_success(
        pk().args(["--dry-run", "installed"]),
        "pk --dry-run installed",
    );
}

#[test]
fn outdated_dry_run()
{
    assert_success(
        pk().args(["--dry-run", "outdated"]),
        "pk --dry-run outdated",
    );
}

#[test]
fn sync_dry_run()
{
    assert_success(
        pk().args(["--dry-run", "sync"]),
        "pk --dry-run sync",
    );
}

#[test]
fn cleanup_dry_run()
{
    assert_success(
        pk().args(["--dry-run", "cleanup"]),
        "pk --dry-run cleanup",
    );
}

// ── Needsfile satisfaction (dry-run) ────────────────────────────

#[test]
fn satisfy_dry_run_with_valid_needsfile()
{
    let dir = std::env::temp_dir().join("pk-regression");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("smoke.Needsfile");
    std::fs::write(
        &path,
        "ripgrep\nif os = linux {\n    curl\n}\n",
    )
    .unwrap();

    assert_success(
        pk().args([
            "--dry-run", "--quiet",
            "satisfy",
            path.to_str().unwrap(),
        ]),
        "pk --dry-run satisfy",
    );

    std::fs::remove_file(path).ok();
}

#[test]
fn satisfy_malformed_needsfile_exits_nonzero()
{
    let dir = std::env::temp_dir().join("pk-regression");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bad.Needsfile");
    std::fs::write(&path, "if os = linux {\n    curl\n").unwrap();

    assert_failure(
        pk().args([
            "--dry-run", "--quiet",
            "satisfy",
            path.to_str().unwrap(),
        ]),
        "pk satisfy malformed",
    );

    std::fs::remove_file(path).ok();
}

// ── SBOM (dry-run) ──────────────────────────────────────────────

#[test]
fn sbom_dry_run()
{
    assert_success(
        pk().args(["--dry-run", "sbom"]),
        "pk --dry-run sbom",
    );
}

#[test]
fn sbom_spdx_format_dry_run()
{
    assert_success(
        pk().args(["--dry-run", "sbom", "--format", "spdx"]),
        "pk --dry-run sbom --format spdx",
    );
}

// ── Global flags ────────────────────────────────────────────────

#[test]
fn mode_and_feature_flags_accepted()
{
    assert_success(
        pk().args([
            "--dry-run", "--quiet",
            "--mode", "dev",
            "--feature", "docs",
            "inventory",
        ]),
        "pk --mode dev --feature docs inventory",
    );
}
