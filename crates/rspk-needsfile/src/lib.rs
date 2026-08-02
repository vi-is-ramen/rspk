//! Needsfile parser for declarative package dependencies.
//!
//! A Needsfile is a text file that lists packages to be installed.
//! Entries can be unconditional or grouped under conditional blocks
//! with a boolean expression language.
//!
//! # Syntax
//!
//! ```text
//! # Comment
//! package                          # unconditional
//! package=1.2.3                    # with version
//! manager:package                  # force manager
//! manager:package=1.2.3            # both
//!
//! if os = linux && present "apt" {
//!     apt:curl=8.4.0
//! }
//!
//! if os = windows || os = macos {
//!     brew:git
//! }
//!
//! if feature "docs" {
//!     rustdoc-json
//! }
//!
//! if mode = "dev" {
//!     cargo-watch=0.8.0
//!     cargo-nextest
//! }
//! ```
//!
//! # Condition language
//!
//! Conditions support the following primitives:
//!
//! - `os = <platform>` — current platform equals the given value (`linux`,
//!   `macos`, `windows`, `freebsd`, `openbsd`, `netbsd`, `dragonfly`,
//!   `android`).
//! - `present "<manager>"` — the named manager is available on the system.
//! - `feature "<name>"` — the named feature is enabled (passed via `--feature`
//!   on the CLI).
//! - `mode = "<value>"` — the current mode matches (passed via `--mode` on the
//!   CLI).
//!
//! Connectives: `&&` (and), `||` (or), `!` (not), parentheses.
//!
//! `=` and `==` are both accepted for equality.
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

mod error;
mod eval;
mod lexer;
mod parser;
mod types;

pub use error::{NeedsfileError, ParseError};
pub use eval::flatten;
pub use types::{
    Condition, ConditionalBlock, EvalContext, NeedsEntry, NeedsItem,
};

use std::path::Path;

/// Parses a Needsfile and returns the raw AST.
///
/// For most callers, prefer [`resolve_needsfile`] which additionally
/// evaluates conditions.
pub fn parse_needsfile<P: AsRef<Path>>(
    path: P,
) -> Result<Vec<NeedsItem>, NeedsfileError>
{
    let path_str = path.as_ref().display().to_string();
    let source =
        std::fs::read_to_string(path.as_ref()).map_err(|e| NeedsfileError {
            source:   String::new(),
            path:     path_str.clone(),
            errors:   Vec::new(),
            io_error: Some(e),
        })?;
    let tokens = match lexer::tokenize(&source)
    {
        Ok(t) => t,
        Err(e) =>
        {
            return Err(NeedsfileError {
                source,
                path: path_str,
                errors: vec![e],
                io_error: None,
            });
        },
    };
    let mut p = parser::Parser::new(tokens);
    match p.parse_file()
    {
        Ok(items) => Ok(items),
        Err(e) => Err(NeedsfileError {
            source,
            path: path_str,
            errors: vec![e],
            io_error: None,
        }),
    }
}

/// Parses a Needsfile, evaluates its conditional blocks against `ctx`,
/// and returns a flat list of [`NeedsEntry`]s ready for installation.
pub fn resolve_needsfile<P: AsRef<Path>>(
    path: P,
    ctx: &EvalContext,
) -> Result<Vec<NeedsEntry>, NeedsfileError>
{
    let items = parse_needsfile(path)?;
    Ok(flatten(&items, ctx))
}

#[cfg(test)]
mod tests
{
    use super::*;
    use rspk_core::Platform;
    use std::collections::HashSet;
    use std::fs;

    fn empty_ctx() -> EvalContext
    {
        EvalContext::default()
    }

    fn linux_ctx() -> EvalContext
    {
        EvalContext {
            os:                 Some(Platform::Linux),
            available_managers: ["apt", "cargo"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            features:           HashSet::new(),
            mode:               None,
        }
    }

    #[test]
    fn test_parse_simple_package()
    {
        let source = "ripgrep\n";
        let tokens = lexer::tokenize(source).unwrap();
        let items = parser::Parser::new(tokens).parse_file().unwrap();
        assert_eq!(items.len(), 1);
        match &items[0]
        {
            NeedsItem::Entry(e) =>
            {
                assert_eq!(e.package, "ripgrep");
                assert_eq!(e.manager, None);
                assert_eq!(e.version, None);
            },
            _ => panic!("expected Entry"),
        }
    }

    #[test]
    fn test_parse_with_manager_and_version()
    {
        let source = "npm:@angular/core=16.0.0\n";
        let tokens = lexer::tokenize(source).unwrap();
        let items = parser::Parser::new(tokens).parse_file().unwrap();
        match &items[0]
        {
            NeedsItem::Entry(e) =>
            {
                assert_eq!(e.package, "@angular/core");
                assert_eq!(e.manager.as_deref(), Some("npm"));
                assert_eq!(e.version.as_deref(), Some("16.0.0"));
            },
            _ => panic!("expected Entry"),
        }
    }

    #[test]
    fn test_parse_conditional_block()
    {
        let source = "if os = linux {\n    apt:curl\n}\n";
        let tokens = lexer::tokenize(source).unwrap();
        let items = parser::Parser::new(tokens).parse_file().unwrap();
        assert_eq!(items.len(), 1);
        match &items[0]
        {
            NeedsItem::Conditional(block) =>
            {
                assert_eq!(block.condition, Condition::OsEq("linux".into()));
                assert_eq!(block.items.len(), 1);
            },
            _ => panic!("expected Conditional"),
        }
    }

    #[test]
    fn test_eval_os_eq_true()
    {
        let cond = Condition::OsEq("linux".into());
        assert!(cond.eval(&linux_ctx()));
    }

    #[test]
    fn test_eval_os_eq_false()
    {
        let cond = Condition::OsEq("windows".into());
        assert!(!cond.eval(&linux_ctx()));
    }

    #[test]
    fn test_eval_present()
    {
        let cond = Condition::ManagerPresent("apt".into());
        assert!(cond.eval(&linux_ctx()));
        let cond = Condition::ManagerPresent("brew".into());
        assert!(!cond.eval(&linux_ctx()));
    }

    #[test]
    fn test_eval_feature()
    {
        let mut ctx = empty_ctx();
        ctx.features.insert("docs".into());
        assert!(Condition::FeaturePresent("docs".into()).eval(&ctx));
        assert!(!Condition::FeaturePresent("other".into()).eval(&ctx));
    }

    #[test]
    fn test_eval_mode()
    {
        let mut ctx = empty_ctx();
        ctx.mode = Some("dev".into());
        assert!(Condition::ModeEq("dev".into()).eval(&ctx));
        assert!(!Condition::ModeEq("prod".into()).eval(&ctx));
    }

    #[test]
    fn test_eval_not_and_or()
    {
        let ctx = linux_ctx();
        let linux = Condition::OsEq("linux".into());
        let windows = Condition::OsEq("windows".into());
        assert!(!Condition::Not(Box::new(linux.clone())).eval(&ctx));
        assert!(
            Condition::Or(Box::new(linux.clone()), Box::new(windows.clone()))
                .eval(&ctx)
        );
        assert!(!Condition::And(Box::new(linux), Box::new(windows)).eval(&ctx));
    }

    #[test]
    fn test_eval_not_equals_desugars()
    {
        let source = "if os != windows {\n    curl\n}\n";
        let tokens = lexer::tokenize(source).unwrap();
        let items = parser::Parser::new(tokens).parse_file().unwrap();
        let linux = linux_ctx();
        let entries = flatten(&items, &linux);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].package, "curl");
    }

    #[test]
    fn test_flatten_skips_false_branch()
    {
        let source = "if os = windows {\n    choco:git\n}\napt:curl\n";
        let tokens = lexer::tokenize(source).unwrap();
        let items = parser::Parser::new(tokens).parse_file().unwrap();
        let entries = flatten(&items, &linux_ctx());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].package, "curl");
    }

    #[test]
    fn test_flatten_nested_blocks()
    {
        let source = r#"
if os = linux {
    if present "apt" {
        apt:ripgrep
    }
    if present "dnf" {
        dnf:ripgrep
    }
}
"#;
        let tokens = lexer::tokenize(source).unwrap();
        let items = parser::Parser::new(tokens).parse_file().unwrap();
        let entries = flatten(&items, &linux_ctx());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].package, "ripgrep");
        assert_eq!(entries[0].manager.as_deref(), Some("apt"));
    }

    #[test]
    fn test_combined_and_or()
    {
        let source =
            "if (os = linux && present \"apt\") || os = macos {\n    curl\n}\n";
        let tokens = lexer::tokenize(source).unwrap();
        let items = parser::Parser::new(tokens).parse_file().unwrap();
        let entries = flatten(&items, &linux_ctx());
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_resolve_needsfile_roundtrip()
    {
        let content = r#"# tools
ripgrep
if os = linux {
    apt:curl=8.4.0
}
if feature "docs" {
    cargo:rustdoc-json
}
"#;
        let dir = std::env::temp_dir();
        let path = dir.join("test.needsfile");
        fs::write(&path, content).unwrap();
        let mut ctx = linux_ctx();
        ctx.features.insert("docs".into());
        let entries = resolve_needsfile(&path, &ctx).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].package, "ripgrep");
        assert_eq!(entries[1].package, "curl");
        assert_eq!(entries[1].version.as_deref(), Some("8.4.0"));
        assert_eq!(entries[2].package, "rustdoc-json");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_error_unclosed_block()
    {
        let source = "if os = linux {\n    curl\n";
        let tokens = lexer::tokenize(source).unwrap();
        let err = parser::Parser::new(tokens).parse_file().unwrap_err();
        assert!(err.message.contains("unclosed"));
    }

    #[test]
    fn test_error_unknown_keyword()
    {
        let source = "if blah = 1 {\n}\n";
        let tokens = lexer::tokenize(source).unwrap();
        let err = parser::Parser::new(tokens).parse_file().unwrap_err();
        assert!(err.message.contains("unknown condition"));
    }

    #[test]
    fn test_error_unterminated_string()
    {
        let source = "if feature \"oops {\n}\n";
        let err = lexer::tokenize(source).unwrap_err();
        assert!(err.message.contains("unterminated"));
    }
}
