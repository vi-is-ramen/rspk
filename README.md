# Pk

<a href="https://xkcd.com/1654/" title="XKCD #1654: Universal Install Script">
  <img align="right" width="220" src="https://imgs.xkcd.com/comics/universal_install_script.png" alt="XKCD #1654"/>
</a>

![GitHub top language](https://img.shields.io/github/languages/top/vi-is-ramen/rspk)
[![crates.io](https://img.shields.io/crates/v/rspk-cli.svg)](https://crates.io/crates/rspk-cli)
[![docs.rs](https://img.shields.io/docsrs/rspk-api)](https://docs.rs/rspk-api)
[![Build Status](https://github.com/vi-is-ramen/rspk/actions/workflows/ci.yml/badge.svg)](https://github.com/vi-is-ramen/rspk/actions)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Pk is a universal install script. Think [`yt-dlp`](https://github.com/yt-dlp/yt-dlp), but for package managers instead of videos.

It wraps 25+ package managers behind a single CLI so you never have to remember whether the flag is `-S`, `install`, `add`, or `brew install --cask`. You write what you need; Pk figures out how to install it on whatever system you happen to be on.

This is a direct answer to [XKCD #1654](https://xkcd.com/1654/). Unlike
[MPM](https://github.com/kdeldycke/meta-package-manager/), Pk respects minimalism,
security, perfomance and portablitiy. It runs on every platform Rust can build to.

And yes, this makes it yet another instance of [XKCD #927](https://xkcd.com/927/). We are sorry. We are also not sorry.

---

## Why

Every project README starts with:

```shell
# Debian/Ubuntu
sudo apt install ripgrep

# Fedora
sudo dnf install ripgrep

# Arch
sudo pacman -S ripgrep

# macOS
brew install ripgrep

# Windows
winget install BurntSushi.ripgrep.MSVC

# ...or just
cargo install ripgrep
```

Now multiply that by every dependency, every CI runner, every teammate's machine,
every BSD box in the corner that nobody wants to touch. Pk collapses all of that into:

```shell
pk install ripgrep
```

Pk discovers which package managers are available, resolves the package name across
ecosystems (via Repology, crates.io, AUR, RubyGems), and picks the best candidate by
priority: system managers first, then universal managers (flatpak, snap),
then language-specific ones (cargo, npm), then AUR helpers.

---

## Installation

From crates.io (any platform with Rust 1.85+):

```shell
cargo install rspk-cli
```

Native packages are published on every release:

| Platform | Format | Install |
|---|---|---|
| Debian / Ubuntu | `.deb` | `sudo dpkg -i pk_*.deb` |
| Fedora / RHEL | `.rpm` | `sudo dnf install pk-*.rpm` |
| Arch Linux | `.pkg.tar.zst` | `sudo pacman -U pk-*.pkg.tar.zst` |
| Alpine Linux | `.apk` | `sudo apk add --allow-untrusted pk-*.apk` |
| Windows | `.msi` | Run the installer |

Pre-built binaries (tar.gz / zip) are attached to every [GitHub Release](https://github.com/vi-is-ramen/rspk/releases).

Verify checksums:

```shell
sha256sum -c SHA256SUMS
```

---

## Usage

### Basics

```shell
pk inventory                  # list discovered package managers
pk installed                  # list installed packages across all managers
pk outdated                   # list packages with available updates

pk install ripgrep            # auto-detect manager, resolve name, install
pk install curl --manager apt # force a specific manager
pk install lodash=4.17.21     # pin a version
pk install @angular/core      # npm scoped packages work too

pk upgrade                    # upgrade everything
pk upgrade curl               # upgrade one package
pk uninstall wget

pk search "json parser"       # search across managers
pk resolve curl               # show how each manager would resolve "curl"

pk sync                       # refresh repository indexes
pk cleanup                    # clean caches, remove orphans
```

Global flags:

```shell
pk --dry-run install curl     # print commands without executing
pk --quiet install curl       # auto-select manager, no prompts
```

### Needsfile

A Needsfile is a declarative list of what a project needs. It supports
conditional blocks gated on OS, available managers, feature flags, and modes.

```text
# Base tools — always installed
ripgrep
fd-find

# OS-specific
if os = linux && present "apt" {
    apt:curl=8.4.0
}

if os = macos {
    brew:curl
}

if os = windows {
    winget:Git.Git
}

# Feature-gated (enabled via --feature)
if feature "docs" {
    cargo:mdbook
}

# Mode-gated (enabled via --mode)
if mode = "dev" {
    cargo:cargo-nextest
    cargo:cargo-llvm-cov
}

if mode = "prod" {
    cargo:cargo-dist
}

# Nested conditions work
if os = linux {
    if present "pacman" {
        pacman:ripgrep
    }
}

# Negation
if !os = windows {
    cargo:bacon
}
```

Satisfy it:

```shell
pk satisfy Needsfile
pk --mode dev --feature docs satisfy Needsfile
pk --dry-run --quiet satisfy Needsfile
```

The condition language supports `&&`, `||`, `!`, and parentheses. Primitives:

| Condition | Meaning |
|---|---|
| `os = linux` | Current OS matches (`linux`, `macos`, `windows`, `freebsd`, `openbsd`, `netbsd`, `dragonfly`, `android`) |
| `present "apt"` | Manager `apt` was discovered on this system |
| `feature "docs"` | `--feature docs` was passed |
| `mode = "dev"` | `--mode dev` was passed |

### SBOM generation

`pk` can produce a Software Bill of Materials for everything installed on the system, in
either CycloneDX 1.6 or SPDX 2.3 format. Every component carries a
[PURL](https://github.com/package-url/purl-spec) for unambiguous identification.

```shell
pk sbom                              # CycloneDX to stdout
pk sbom --format spdx -o sbom.json   # SPDX to file
pk sbom --manager apt                # restrict to one manager
pk sbom --component-name my-app --component-version 2.1.0
```

The output is compatible with OWASP Dependency-Track, Trivy, Grype, and
anything else that speaks CycloneDX or SPDX.

### JSON-RPC server

`pk` can run as a long-lived subprocess driven over stdio with newline-delimited JSON-RPC 2.0:

```shell
pk rpc
```

```json
>>> {"jsonrpc":"2.0","method":"inventory","id":1}
<<< {"jsonrpc":"2.0","result":{"managers":[...]},"id":1}

>>> {"jsonrpc":"2.0","method":"install","params":{"package":"ripgrep"},"id":2}
<<< {"jsonrpc":"2.0","result":{"installed":true,"manager":"apt","output":"..."},"id":2}
```

Available methods: `inventory`, `installed`, `outdated`, `search`, `resolve`, `install`,
`upgrade`, `uninstall`, `sync`, `cleanup`, `satisfy`, `sbom`, `system.listMethods`, `system.describe`.

Batch requests (arrays) are supported. This is the intended integration point for GUIs, IDE plugins, and orchestration scripts.

---

## Supported package managers

| Manager     | ID            | Platforms                | Priority  |
| ----------- | ------------- | ------------------------ | --------- |
| APT         | `apt`         | Linux (Debian, Ubuntu)   | system    |
| Aptitude    | `aptitude`    | Linux (Debian)           | system    |
| apk         | `apk`         | Linux (Alpine)           | system    |
| DNF         | `dnf`         | Linux (Fedora, RHEL 8+)  | system    |
| YUM         | `yum`         | Linux (RHEL 7, CentOS 7) | system    |
| Zypper      | `zypper`      | Linux (openSUSE, SLE)    | system    |
| XBPS        | `xbps`        | Linux (Void)             | system    |
| pacman      | `pacman`      | Linux (Arch)             | system    |
| Homebrew    | `brew`        | macOS, Linux             | system    |
| MacPorts    | `macports`    | macOS                    | system    |
| FreeBSD pkg | `freebsd-pkg` | FreeBSD, DragonFly BSD   | system    |
| OpenBSD pkg | `openbsd-pkg` | OpenBSD                  | system    |
| pkgin       | `pkgin`       | NetBSD                   | system    |
| Termux pkg  | `termux-pkg`  | Android                  | system    |
| Flatpak     | `flatpak`     | Linux                    | universal |
| Snap        | `snap`        | Linux                    | universal |
| Scoop       | `scoop`       | Windows                  | universal |
| Cargo       | `cargo`       | all                      | language  |
| npm         | `npm`         | all                      | language  |
| RubyGems    | `gems`        | all                      | language  |
| Nix         | `nix`         | Linux, macOS             | language  |
| winget      | `winget`      | Windows                  | system    |
| Chocolatey  | `choco`       | Windows                  | system    |
| yay         | `yay`         | Linux (Arch, AUR)        | auxiliary |
| paru        | `paru`        | Linux (Arch, AUR)        | auxiliary |

If your manager is missing, you can influence its implementation: [document its output](https://github.com/vi-is-ramen/rspk/issues/new?template=new-package-manager.yml) or submit a pull request. Each manager is a single file implementing one trait — the barrier to entry is low.

---

## Architecture

The project is a Cargo workspace with nine crates. The dependency graph flows strictly downward:

```
rspk-cli (binary: pk)
+-- rspk-rpc          JSON-RPC 2.0 server over stdio
+-- rspk-telemetry    OpenTelemetry traces, metrics, logs (OTLP)
+-- rspk-api          Business logic: resolver, Needsfile satisfaction
|   +-- rspk-needsfile    Needsfile lexer, parser, condition evaluator
|   \-- rspk-managers     25+ PackageManager implementations
|       +-- rspk-regs         Registry API clients (Repology, crates.io, AUR, RubyGems)
|       \-- rspk-executor     Command spawning, progress parsing
\-- rspk-core         Traits: PackageManager, Package, Platform, ProgressReporter
```

| Crate            | Purpose                                                                                                           |
| ---------------- | ----------------------------------------------------------------------------------------------------------------- |
| `rspk-core`      | Foundational traits and types. Zero external dependencies beyond serde/semver.                                    |
| `rspk-executor`  | `CommandBuilder` for safe process spawning with dry-run, sudo, timeout, and live progress parsing.                |
| `rspk-regs`      | HTTP clients for Repology, crates.io, AUR RPC, and RubyGems. Used for cross-ecosystem name resolution.            |
| `rspk-managers`  | One module per package manager. Each implements `PackageManager` and parses the manager's native output format.   |
| `rspk-needsfile` | Hand-written lexer + recursive-descent parser for the Needsfile format, with `annotate-snippets` error rendering. |
| `rspk-api`       | Cross-cutting logic shared by CLI and RPC: parallel candidate resolution, Needsfile satisfaction pipeline.        |
| `rspk-telemetry` | Optional OpenTelemetry integration. Disabled by default; enable with `PK_TELEMETRY=1`.                            |
| `rspk-rpc`       | Newline-delimited JSON-RPC 2.0 server. Full spec compliance including batch requests and standard error codes.    |
| `rspk-cli`       | The `pk` binary. Clap-based CLI, indicatif progress bars, dialoguer prompts.                                      |

Design decisions worth noting:

- **Async everywhere.** Manager discovery, package resolution, and multi-manager installation all run concurrently via Tokio `JoinSet`. Within a single manager, operations are sequential to avoid lock conflicts (dpkg and rpm do not appreciate concurrency).
- **Progress is UI-agnostic.** Managers emit progress through a `ProgressReporter` trait. The CLI renders indicatif bars; RPC mode uses a no-op reporter so stdout stays clean JSON.
- **Dry-run is a first-class citizen.** Every command respects `--dry-run` and prints the exact shell commands it would execute, with proper escaping.
- **Allman brace style.** Yes, really. See `rustfmt.toml`. The author has spent too many years in C++ codebases and has made peace with it.

---

## Telemetry

Pk can export traces, metrics, and structured logs to any OTEL-compatible backend (Grafana Tempo/Loki, Jaeger, Datadog, Prometheus) via OTLP.

Telemetry is **off by default**. Enable it explicitly:

```shell
PK_TELEMETRY=1 OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 pk install curl
```

| Variable                      | Effect                                          |
| ----------------------------- | ----------------------------------------------- |
| `PK_TELEMETRY=1`              | Enable trace and metric export                  |
| `PK_TELEMETRY_LOGS=1`         | Also export structured logs                     |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | Collector URL (default `http://localhost:4317`) |
| `OTEL_SERVICE_NAME`           | Service name (default `pk`)                     |

Exported metrics include `pk.operations.total`, `pk.manager.call.duration_seconds`,
`pk.packages.installed`, and `pk.errors.total`, all tagged with manager and command labels.

---

## Building from source

Requires Rust 1.85+ (edition 2024).

```shell
git clone https://github.com/vi-is-ramen/rspk.git
cd rspk

cargo build                     # debug
cargo build --release           # release (thin LTO, stripped)
cargo build --profile release-small  # size-optimized (fat LTO, opt-level=z)

cargo test --all-features       # run all tests
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Cross-compilation targets (via `cross`):

```shell
cross build --release --target x86_64-unknown-linux-gnu
cross build --release --target x86_64-unknown-linux-musl
cross build --release --target aarch64-unknown-linux-gnu
cross build --release --target x86_64-apple-darwin
cross build --release --target aarch64-apple-darwin
cross build --release --target x86_64-pc-windows-msvc
```

A `Justfile` is provided for convenience: `just build`, `just test`, `just check`, `just dist`, `just size`.

---

## FAQ

**Why are the crates named `rspk-*` but the project is called "Pk"?**

Someone reserved the name `pk` on crates.io roughly half a year before this project existed.
The binary is still `pk`. The crates are not. Life goes on.

**Why async? The README of the original MPM says one HTTP request per invocation does not justify it.**

Fair point, and for a single `pk install` it is indeed overkill. But Pk does parallel manager discovery,
parallel cross-ecosystem resolution, and parallel multi-manager installation. When satisfying a Needsfile
with entries across apt, cargo, and npm simultaneously, async stops being overkill and starts being the
obvious choice.

**Why Allman style?**

First: K&R is the default, not the standard.
Second: the author has written a lot of C++ where Allman is far more common,
and old habits die hard. The `rustfmt.toml` is the end of this discussion.

**Why is BSD support listed but the original FAQ said it was not supported?**

The original FAQ was written before FreeBSD, OpenBSD, NetBSD, DragonFly, and Termux/Android support landed.
They are supported now. If you find a bug on your favourite BSD, that is a feature request with extra
steps — file an issue.

**Can Pk remove packages? Upgrade? Search?**

Yes. Pk supports the full lifecycle: `install`, `upgrade`, `uninstall`, `search`, `resolve`, `sync`, `cleanup`,
`installed`, `outdated`. What it does not do is dependency resolution across managers, transactional rollbacks,
or repository management. For that, use your system manager directly.

---

## Contributing

- Bug reports: [bug-report.yml](https://github.com/vi-is-ramen/rspk/issues/new?template=bug-report.yml)
- New manager requests: [new-package-manager.yml](https://github.com/vi-is-ramen/rspk/issues/new?template=new-package-manager.yml)
- Code of conduct: [Contributor Covenant 1.4](.github/code-of-conduct.md)

If you are adding a new manager: implement the `PackageManager` trait in a new file under `crates/rspk-managers/src/`,
register it in `lib.rs`, and add tests that parse real output from the manager. The existing implementations are the best documentation.

---

## License

[MIT](LICENSE). Do what you want. Attribution is appreciated but not required.

---

Made with ❤️ for Rust, for package maintainers, and for everyone who has ever
typed `sudo apt install` on an Arch machine and watched the world burn.
