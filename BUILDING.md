# Building Shadow Convert

## Dependencies (Debian / Ubuntu / Pop!_OS)

```bash
sudo apt install \
  build-essential pkg-config curl \
  libgtk-4-dev libadwaita-1-dev \
  ffmpeg imagemagick \
  librsvg2-bin desktop-file-utils
```

Rust 1.75+ with Cargo is required (`rustc` / `cargo`).

Fedora equivalents: `gtk4-devel`, `libadwaita-devel`, `ffmpeg`, `ImageMagick`, `librsvg2-tools`.

## Build

```bash
cargo build --release
./target/release/shadow-convert --version
./target/release/shadow-convert --detect-hw
```

Icons:

```bash
./scripts/generate-icons.sh
```

## Tests

Tests create short synthetic media with FFmpeg and run real conversions.

```bash
cargo test
```

## User install

```bash
./scripts/install-user.sh
desktop-file-validate ~/.local/share/applications/com.shadowfetch.Convert.desktop
```

## Debian package

```bash
./scripts/build-deb.sh
```

The script writes `dist/shadow-convert_1.0.0_<arch>.deb`. Installing that package system-wide requires `sudo dpkg -i`. You do not need root to *build* the package or to install with `install-user.sh`.

`cargo-deb` is optional. If it is on `PATH`, the script prefers it; otherwise it assembles the archive with `dpkg-deb`.

## Format and lint

If `rustfmt` and `clippy` are installed:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

System `rustc` packages may not ship these components. The tree is kept clippy-clean when they are available.
