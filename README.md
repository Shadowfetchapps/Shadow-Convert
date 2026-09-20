# Shadow Convert

**Convert video, audio, and images locally on Linux.**

Drop a file, pick an action, convert. Shadow Convert is a native GTK4 / libadwaita desktop app. It is not a website, not a localhost server, and not Electron.

![Shadow Convert icon](data/icons/hicolor/128x128/apps/shadow-convert.png)

## What it does

- **Video** — MP4, MKV, WebM; 1080p / 1440p / 4K (never silent-upscale); compress; extract audio; remove audio; trim; GIF
- **Audio** — MP3, WAV, FLAC, AAC, Opus; normalize loudness; trim
- **Images** — PNG, JPEG, WebP, AVIF; resize without upscaling; compress
- Probes the file with ffprobe and inspects content, not just the extension
- Quality presets: Small, Balanced, **High (default)**, Maximum
- Hardware encoding (NVENC H.264 / HEVC / AV1) when FFmpeg exposes it and the job is long enough to benefit; CPU fallback is always available
- Stream-copy / remux when the codecs already fit the target container
- Progress with ETA when FFmpeg reports time; Cancel stops child processes
- Originals are never overwritten
- Offline. No accounts, ads, telemetry, or cloud

## Screenshots

Screenshots belong in `docs/screenshots/` and must not include private filenames or home-directory paths. If this folder is empty, the app still builds and runs; capture them locally after install if you want gallery images.

## Install (user, no root)

```bash
git clone https://github.com/ShadowfetchLinux/Shadow-Convert.git
cd Shadow-Convert
cargo build --release
./scripts/install-user.sh
```

This copies the binary to `~/.local/bin/shadow-convert`, the desktop file to `~/.local/share/applications/`, and icons to `~/.local/share/icons/hicolor/`.

Launch from the app menu or:

```bash
shadow-convert
```

A `.deb` is produced with `./scripts/build-deb.sh` (see [BUILDING.md](BUILDING.md)). System-wide `dpkg -i` needs administrator rights; the user install does not.

## Requirements

- Linux desktop with GTK 4 and libadwaita
- FFmpeg / ffprobe (video and audio)
- ImageMagick `convert` (preferred for images; FFmpeg is the fallback)

Optional: an FFmpeg build with NVENC, libwebp, libaom-av1, libmp3lame, libopus.

## Limitations (honest)

- AVIF depends on ImageMagick AVIF support or FFmpeg `libaom-av1`
- ICC color profiles are preserved when ImageMagick can do so; some FFmpeg still-image paths flatten toward a typical display encoding
- Size estimates are approximate
- Multi-file drops open the first file; use [Shadow Batch Processor](https://github.com/ShadowfetchLinux/Shadow-Batch-Processor) for bulk jobs
- GIF conversion re-encodes and can be slow on long clips
- Hardware encode is detected at runtime. It is never assumed from a particular GPU

## License

MIT. See [LICENSE](LICENSE).
