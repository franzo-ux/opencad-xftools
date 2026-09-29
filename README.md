# OpenCADStudio Point Numbering

> **Experimental / work in progress.** This plugin is provided for testing and evaluation. Validate the output on copies of your drawings before using it in production.

An [OpenCADStudio](https://github.com/HakanSeven12/OpenCADStudio) plugin that places incrementing text labels by clicking points in the drawing.

- [Guida rapida in italiano](docs/guide.it.md)
- [Quick guide in English](docs/guide.en.md)

## Install

This plugin is built for OpenCADStudio `v2026.39` and Rust `1.98.1`.

### Windows release

Download `opencad.point_numbering-windows-x86_64.dll` and `plugin.toml` from the [latest release](https://github.com/franzo-ux/opencad-xftools/releases/latest), then copy both files to:

```text
%APPDATA%\OpenCADStudio\plugins\opencad.point_numbering\
```

Restart OpenCADStudio.

### macOS (Apple Silicon)

Download `opencad.point_numbering-macos-aarch64.dylib` and `plugin.toml` from the [latest release](https://github.com/franzo-ux/opencad-xftools/releases/latest), then copy both files to:

```text
~/Library/Application Support/OpenCADStudio/plugins/opencad.point_numbering/
```

Restart OpenCADStudio.

### Linux (x86_64) release

Download `opencad.point_numbering-linux-x86_64.so` and `plugin.toml` from the [latest release](https://github.com/franzo-ux/opencad-xftools/releases/latest), then copy both files to:

```text
~/.config/OpenCADStudio/plugins/opencad.point_numbering/
```

Restart OpenCADStudio.

### Linux source build

```sh
cargo build --release
mkdir -p ~/.config/OpenCADStudio/plugins/opencad.point_numbering
cp target/release/libopencad_point_numbering.so \
  ~/.config/OpenCADStudio/plugins/opencad.point_numbering/
cp plugin.toml ~/.config/OpenCADStudio/plugins/opencad.point_numbering/
```

## Use

Click **Number points** in the **xfTools** ribbon tab, or run `PNUM`. On Windows and macOS, a compact settings window lets you set the start number, increment, prefix, text height, upper-right offset, and text style before clicking points. Linux keeps its existing inline configuration.

The command accepts vertices and empty drawing locations. It creates a text label using the selected settings, then retains the next number until OpenCADStudio closes. Press Enter or Esc to finish.

### PDF reference points

After attaching a PDF underlay with OpenCADStudio, click **PDF reference points** or run `PDFREFS`. xfTools reads blue vector strokes in every attached PDF and creates CAD `POINT` entities at their vertices. Use them as native snap references; raster-only content cannot be detected.

### Enclose existing text

Click **Enclose text** or run `TFRAME`, choose a rectangle, circle, or slot, then choose **Fit** (text bounds plus offset) or **Fixed** dimensions. Select existing `TEXT` entities one by one; press Enter or Esc to finish. On Linux, provide settings inline: `TFRAME:rectangle,fit,1,10,5`.

### Settings window

The Windows and macOS form runs separately from OpenCADStudio, so it does not inherit the host theme automatically. It is intentionally compact and utility-focused. A future visual pass can align its palette, typography, spacing, and accent color with a chosen OpenCADStudio theme.

## Development

```sh
cargo test
cargo build --release
```

## License

GPL-3.0-only. See [LICENSE](LICENSE).
