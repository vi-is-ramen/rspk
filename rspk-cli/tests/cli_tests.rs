use assert_cmd::Command;
use assert_fs::prelude::*;
use predicates::prelude::*;

#[test]
fn test_cli_no_args()
{
    let mut cmd = Command::cargo_bin("pk").unwrap();
    cmd.assert().failure().stderr(predicate::str::contains(
        "required arguments were not provided",
    ));
}

#[test]
fn test_cli_help()
{
    let mut cmd = Command::cargo_bin("pk").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Universal package installer"));
}

#[test]
fn test_cli_file_reading()
{
    let temp = assert_fs::TempDir::new().unwrap();
    let deps_file = temp.child("deps.txt");
    deps_file.write_str("# Comment\ngit\n\ncurl\n").unwrap();

    let mut cmd = Command::cargo_bin("pk").unwrap();

    // Pass an invalid manager to force a quick failure without actually
    // installing packages
    cmd.arg(format!("@{}", deps_file.path().to_string_lossy()))
        .arg("--manager")
        .arg("nonexistent_mgr");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Unknown manager"));
}

#[test]
fn test_cli_invalid_manager()
{
    let mut cmd = Command::cargo_bin("pk").unwrap();
    cmd.arg("git").arg("--manager").arg("fake_manager");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Unknown manager"));
}
