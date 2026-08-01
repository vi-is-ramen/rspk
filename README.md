# Rspk

<a href="https://xkcd.com/1654/" alt="XKCD #1654: Universal Install Script">
<img align="right" width="20%" height="20%" src="http://imgs.xkcd.com/comics/universal_install_script.png"/>
</a>

![GitHub top language](https://img.shields.io/github/languages/top/vi-is-ramen/rspk)
[![crates.io](https://img.shields.io/crates/v/rspk.svg)](https://crates.io/crates/rspk-core)
![docs.rs](https://img.shields.io/docsrs/rspk-core)
[![Build Status](https://github.com/vi-is-ramen/rspk/actions/workflows/rust.yml/badge.svg)](https://github.com/vi-is-ramen/rspk/actions)

**What is Pk?**

- provides the `pk` CLI, a wrapper around all package managers
- `pk` is like [`yt-dlp`](https://github.com/yt-dlp/yt-dlp), but for package
  managers instead of videos
- `pk` solves [XKCD #1654 - *Universal Install Script*](https://xkcd.com/1654/)
  just like [MPM](https://github.com/kdeldycke/meta-package-manager/) but instead
  creating fully-functional interface abstracting package managers, **Pk** can only
  install packages (as mentioned XKCD#1654: "universal install script", not
  "universal package manager").

---

## Quick start

Thanks to [`Cargo`](https://github.com/rust-lang/cargo), you can install Pk on any platform in one command:

```shell
cargo install rspk-cli
```

## Oops

As mentioned by MPM, it became another pathological case of [XKCD #927: *Standards*](https://xkcd.com/927/).

## Supported Package Managers

So far, we support these package managers:

- Pacman
- Yum
- Yay
- Paru
- Apt
- Aptitude
- Apk
- Pkg
- Rpm
- Dnf
- Zypper
- Winget
- Brew
- Cargo

> [!NOTE]
> If your favorite manager is missing, you can influence its
> implementation: [open a ticket to document its output](https://github.com/vi-is-ramen/rspk/issues/new?assignees=&labels=%F0%9F%8E%81+feature+request&template=new-package-manager.yaml) or submit a pull request.
>
> You can help if you [purchase business support](https://github.com/sponsors/vi-is-ramen)
> or [sponsor the project](https://github.com/sponsors/vi-is-ramen).

## Architecture

This software separated into two different parts: core (librspk/rspk-core) and interface (pk/rspk-cli),

Librspk contains whole logic of the Pk, while pk is just a CLI interface to librspk.

## FAQ

**Q:** Why crates are named `rspk-core` and `rspk-cli` but project named "Pk"?<br>
**A:** 'coz some nasty guy reserved name `pk` in [crates.io](crates.io) about
half of the year ago.<br><br>
**Q:** Why not to use asynchronous HTTP library?<br>
**A:** C'mon, Pk makes literally one network request per invokation. Async is
overkill for it.<br><br>
**Q:** Why Allman style (`rustfmt.toml`) instead K&R which is standard for Rust?<br>
**A:** Firstly, K&R style is not standard. It's either default or recommendation.
Secondly, I have coded a lot in C++ codebases where Allman style is much more usual.<br><br>
**Q:** Why BSD is not supported yet?<br>
**A:** 'coz I (Ivan Chetchasov) don't use BSD and almost haven't experience with it. If you
want, you always can make feature request or pull request. Thanks!

## License

This project is licensed under [MIT license](LICENSE).

---

Made with ❤️ for Rust & maintenance community
