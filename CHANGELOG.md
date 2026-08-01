# Changelog

This file contains changelog of the Pk project.

## Short legend

Each *H2* (except this one) is a version.

Examples: `[1.0.0] - 2024-01-01`, `[9.9.9.9-9] - unreleased`.

Each *H3* is a change description. If it is `+`, it describes addition.
If it is `-`, it describes deletion (must present only in major-changing
versions usually). If it is `~`, it is internal change which does not add
or delete functionality. If it is `!`, it is **VERY** important security
update which **MUST NOT** be ignored and **CAN** introduce breaking changes.

Examples:

### + Added
* Nixpkg support.

### - Removed
* Xbps support.

### ~ Fixed
* Changed caching algorithm.

### ! Security
* Patch for **[CVE-...](#)**.

## [0.1.1] - 2026-08-01

### + Added
* Support of Cargo.

## [0.1.0] - 2026-08-01

### + Added
* Base architecture;
* Support of Pacman;
* Support of Yum;
* Support of Yay;
* Support of Paru;
* Support of Apt;
* Support of Aptitude;
* Support of Apk;
* Support of Pkg;
* Support of Rpm;
* Support of Dnf;
* Support of Zypper;
* Support of Winget;
* Support of Brew;
* CLI interface.
