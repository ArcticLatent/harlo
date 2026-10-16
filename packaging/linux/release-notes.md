## Install

| Your system | Download | Install |
|---|---|---|
| Ubuntu, Debian, Linux Mint, Pop!_OS | `harlo_<version>_amd64.deb` | `sudo apt install ./harlo_*_amd64.deb` |
| Fedora, openSUSE, RHEL-likes | `harlo-<version>-1.x86_64.rpm` | `sudo dnf install ./harlo-*.rpm` (openSUSE: `sudo zypper install ./harlo-*.rpm`) |
| Arch, Manjaro, EndeavourOS, CachyOS | `harlo-<version>-1-x86_64.pkg.tar.zst` | `sudo pacman -U ./harlo-*.pkg.tar.zst` |
| Anything else | `harlo-<version>-x86_64.AppImage` | `chmod +x harlo-*.AppImage && ./harlo-*.AppImage` |

harlo brings its own Qt; it needs glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 or later) and git. Then install the agents you use (Claude Code, OpenCode, pi, Oh My Pi): harlo's setup guide helps on first start.

harlo checks for new versions and updates itself (Settings → General). The files' SHA-256 sums are in `SHA256SUMS`.
