//! Regression: package spec parsing edge cases.
//!
//! `split_package_spec` is the single entry point for parsing
//! user-supplied package identifiers. Its behaviour is contractual.

use rspk_api::split_package_spec;

#[test]
fn plain_name()
{
    assert_eq!(split_package_spec("ripgrep"), ("ripgrep", None));
}

#[test]
fn name_with_version()
{
    assert_eq!(
        split_package_spec("lodash=4.17.21"),
        ("lodash", Some("4.17.21"))
    );
}

#[test]
fn npm_scoped_package()
{
    assert_eq!(split_package_spec("@angular/core"), ("@angular/core", None));
}

#[test]
fn npm_scoped_with_version()
{
    assert_eq!(
        split_package_spec("@angular/core=16.0.0"),
        ("@angular/core", Some("16.0.0"))
    );
}

#[test]
fn version_with_prerelease()
{
    assert_eq!(
        split_package_spec("foo=1.2.3-beta.1"),
        ("foo", Some("1.2.3-beta.1"))
    );
}

#[test]
fn version_with_build_metadata()
{
    assert_eq!(
        split_package_spec("bar=2.0.0+build.42"),
        ("bar", Some("2.0.0+build.42"))
    );
}

#[test]
fn at_sign_in_version_is_not_separator()
{
    // `@` is part of the version, not a separator
    assert_eq!(split_package_spec("pkg=1.0@rc1"), ("pkg", Some("1.0@rc1")));
}

#[test]
fn empty_string()
{
    assert_eq!(split_package_spec(""), ("", None));
}

#[test]
fn only_equals()
{
    assert_eq!(split_package_spec("="), ("", Some("")));
}

#[test]
fn multiple_equals_uses_first()
{
    assert_eq!(split_package_spec("a=b=c"), ("a", Some("b=c")));
}
