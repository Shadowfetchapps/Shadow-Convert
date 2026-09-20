# Architecture

Shadow Convert is a Rust binary plus a library crate. The library owns probing, planning, process execution, and validation. The binary owns the GTK4 / libadwaita window.

```
ui (GTK)  →  probe / plan / ffmpeg / validate  →  FFmpeg or ImageMagick
                 ↑
            settings.json (XDG)
```

## Modules

| Module | Role |
| --- | --- |
| `detect` | Magic-byte sniff so extensions are not trusted alone |
| `probe` | `ffprobe -print_format json` into a typed `Probe` |
| `hw` | Parse `ffmpeg -encoders`; never hardcode a GPU |
| `plan` | Structured argv for each job; remux when codecs already fit |
| `ffmpeg` | Spawn, parse `-progress pipe:1`, cancel via process group |
| `validate` | Probe the output; failed validation fails the job |
| `temps` | Unique cache jobs with a marker file; stale cleanup only for our dirs |
| `settings` | XDG config, quality, hardware preference, theme |
| `ui` | Drop zone, inspect, progress, done, settings, About |

## Safety

- Every external tool is invoked with `Command` arguments. Nothing is interpolated into a shell string.
- Output paths are unique and never equal the source.
- Cancel sends SIGTERM then SIGKILL to the process group so FFmpeg children do not orphan.
- Temporary directories contain `.shadow-convert-temp`. Cleanup never deletes unmarked folders.

## Hardware

NVENC is used only when FFmpeg lists the encoder, the user left “prefer hardware” on, and the clip is long enough that encode time matters. Short jobs and remuxes stay on stream-copy or libx264.

## Color and metadata

ImageMagick `-auto-orient` is preferred for stills so EXIF orientation is applied. Metadata is mapped through unless the user turns preservation off. Web still formats may lose exotic ICC profiles; that limit is documented in the README.
