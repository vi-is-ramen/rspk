//! Regression: Needsfile parser and condition evaluator.
//!
//! Fixed inputs → fixed outputs. If the parser or evaluator changes
//! behaviour, these tests break and force a conscious decision.

use rspk_needsfile::{EvalContext, flatten, parse_needsfile};
use rspk_core::Platform;
use std::collections::HashSet;
use std::io::Write;

/// Writes a Needsfile to a temp file and returns its path.
fn write_tmp(name: &str, content: &str) -> std::path::PathBuf
{
    let dir = std::env::temp_dir().join("pk-regression");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    path
}

fn linux_ctx() -> EvalContext
{
    EvalContext {
        os:                 Some(Platform::Linux),
        available_managers: ["apt", "cargo", "pacman", "dnf"]
            .into_iter()
            .map(String::from)
            .collect::<HashSet<_>>(),
        features:           HashSet::new(),
        mode:               None,
    }
}

// ── Complex Needsfile with all condition types ──────────────────

#[test]
fn complex_needsfile_parses_without_panic()
{
    let path = write_tmp(
        "complex.Needsfile",
        r#"
# Base tools
ripgrep
fd-find

# OS-specific with manager guard
if os = linux && present "apt" {
    apt:curl=8.4.0
    apt:wget
}

if os = macos {
    brew:curl
}

if os = windows || os = macos {
    cargo:cargo-edit
}

# Feature-gated
if feature "lsp" {
    cargo:rust-analyzer
}

# Mode-gated
if mode = "dev" {
    cargo:cargo-nextest
    cargo:cargo-tarpaulin
}

if mode = "prod" {
    cargo:cargo-dist
}

# Nested conditions
if os = linux {
    if present "pacman" {
        pacman:ripgrep
    }
    if present "dnf" {
        dnf:ripgrep
    }
}

# Negation
if !os = windows {
    cargo:bacon
}

# Scoped npm package
npm:@angular/core=16.0.0

# Manager-pinned with version
cargo:serde=1.0.0
"#,
    );

    let items = parse_needsfile(&path).expect("must parse");
    assert!(!items.is_empty(), "AST must not be empty");

    // Evaluate with dev mode + lsp feature
    let mut ctx = linux_ctx();
    ctx = ctx.with_mode("dev").with_feature("lsp");
    let entries = flatten(&items, &ctx);

    // ripgrep, fd-find, apt:curl, apt:wget, cargo:cargo-edit (linux||macos),
    // cargo:rust-analyzer (lsp), cargo:cargo-nextest, cargo:cargo-tarpaulin (dev),
    // pacman:ripgrep, dnf:ripgrep (nested), cargo:bacon (!windows),
    // npm:@angular/core, cargo:serde
    assert!(
        entries.len() >= 12,
        "expected >= 12 entries, got {}",
        entries.len()
    );

    // Spot-check specific entries
    assert!(entries.iter().any(|e| e.package == "ripgrep" && e.manager.is_none()));
    assert!(entries.iter().any(|e| e.package == "curl" && e.manager.as_deref() == Some("apt")));
    assert!(entries.iter().any(|e| e.package == "rust-analyzer"));
    assert!(entries.iter().any(|e| e.package == "bacon"));
    assert!(entries.iter().any(|e| e.package == "@angular/core" && e.manager.as_deref() == Some("npm")));

    std::fs::remove_file(path).ok();
}

// ── Empty Needsfile ─────────────────────────────────────────────

#[test]
fn empty_needsfile_yields_zero_entries()
{
    let path = write_tmp("empty.Needsfile", "# only comments\n\n");
    let items = parse_needsfile(&path).unwrap();
    let entries = flatten(&items, &linux_ctx());
    assert_eq!(entries.len(), 0);
    std::fs::remove_file(path).ok();
}

// ── Malformed Needsfile must fail ───────────────────────────────

#[test]
fn unclosed_block_is_rejected()
{
    let path = write_tmp(
        "unclosed.Needsfile",
        "if os = linux {\n    curl\n",
    );
    let result = parse_needsfile(&path);
    assert!(result.is_err(), "unclosed block must produce an error");
    let err = result.unwrap_err();
    let rendered = err.render();
    assert!(
        rendered.contains("unclosed") || rendered.contains("expected"),
        "error message must mention the problem: {rendered}"
    );
    std::fs::remove_file(path).ok();
}

#[test]
fn unknown_keyword_is_rejected()
{
    let path = write_tmp(
        "unknown_kw.Needsfile",
        "if blah = 1 {\n    curl\n}\n",
    );
    let result = parse_needsfile(&path);
    assert!(result.is_err());
    std::fs::remove_file(path).ok();
}

// ── Condition evaluation stability ──────────────────────────────

#[test]
fn false_branch_is_skipped()
{
    let path = write_tmp(
        "skip.Needsfile",
        "if os = windows {\n    choco:git\n}\napt:curl\n",
    );
    let items = parse_needsfile(&path).unwrap();
    let entries = flatten(&items, &linux_ctx());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].package, "curl");
    std::fs::remove_file(path).ok();
}

#[test]
fn nested_conditions_evaluate_correctly()
{
    let path = write_tmp(
        "nested.Needsfile",
        r#"
if os = linux {
    if present "apt" {
        apt:ripgrep
    }
    if present "brew" {
        brew:ripgrep
    }
}
"#,
    );
    let items = parse_needsfile(&path).unwrap();
    let entries = flatten(&items, &linux_ctx());
    // linux_ctx has apt but not brew
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].manager.as_deref(), Some("apt"));
    std::fs::remove_file(path).ok();
}

#[test]
fn not_equals_desugars_correctly()
{
    let path = write_tmp(
        "noteq.Needsfile",
        "if os != windows {\n    curl\n}\n",
    );
    let items = parse_needsfile(&path).unwrap();
    let entries = flatten(&items, &linux_ctx());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].package, "curl");
    std::fs::remove_file(path).ok();
}

#[test]
fn combined_and_or_precedence()
{
    let path = write_tmp(
        "precedence.Needsfile",
        "if os = linux && present \"apt\" || os = macos {\n    curl\n}\n",
    );
    let items = parse_needsfile(&path).unwrap();

    // linux + apt → true
    let entries = flatten(&items, &linux_ctx());
    assert_eq!(entries.len(), 1);

    // windows, no managers → false
    let win_ctx = EvalContext {
        os:                 Some(Platform::Windows),
        available_managers: HashSet::new(),
        features:           HashSet::new(),
        mode:               None,
    };
    let entries = flatten(&items, &win_ctx);
    assert_eq!(entries.len(), 0);

    std::fs::remove_file(path).ok();
}
