# Packaging

- `scripts/install-user.sh` — copies the binary, desktop file, and icons into `~/.local` (no root).
- `scripts/build-deb.sh` — writes `dist/shadow-convert_1.0.0_<arch>.deb`.

System install of the `.deb` needs `sudo dpkg -i`. Building the archive does not.
