# Quick guide — Point Numbering

> **Experimental:** save a copy of important DWG/DXF files and verify the generated labels before production use.

## What it does

The plugin places numeric text labels in a drawing. Once started, click a vertex or an empty location: a label is placed slightly above and to the right of the click. The next value is kept while OpenCADStudio remains open.

## First installation on Windows

1. Open the [latest release](https://github.com/franzo-ux/opencad-xftools/releases/latest).
2. Download **both** files:
   - `opencad.point_numbering-windows-x86_64.dll`
   - `plugin.toml`
3. Paste this path into File Explorer’s address bar:

   ```text
   %APPDATA%\OpenCADStudio\plugins\opencad.point_numbering\
   ```

4. Create the folder if it does not exist.
5. Copy both files into it.
6. Restart OpenCADStudio.

After restarting, the **xfTools** ribbon tab includes a **Number points** button with a `1,2…` icon.

## Install on macOS (Apple Silicon)

1. Open the [latest release](https://github.com/franzo-ux/opencad-xftools/releases/latest).
2. Download `opencad.point_numbering-macos-aarch64.dylib` and `plugin.toml`.
3. Copy both to:

   ```text
   ~/Library/Application Support/OpenCADStudio/plugins/opencad.point_numbering/
   ```

4. Restart OpenCADStudio.

## Install on Linux (x86_64)

1. Open the [latest release](https://github.com/franzo-ux/opencad-xftools/releases/latest).
2. Download `opencad.point_numbering-linux-x86_64.so` and `plugin.toml`.
3. Copy both to:

   ```text
   ~/.config/OpenCADStudio/plugins/opencad.point_numbering/
   ```

4. Restart OpenCADStudio.

## Standard numbering

1. Open a drawing.
2. Select **xfTools → Number points**, or enter `PNUM` in the command line.
3. Click points in the required order.
4. Press **Enter** or **Esc** to finish.

The first run starts at `1`, increments by `1`, and has no prefix.

## PDF reference points

1. Attach the PDF first with OpenCADStudio’s native command.
2. Select **xfTools → PDF reference points**, or enter `PDFREFS` in the command line.
3. xfTools reads blue vector lines in the PDF and creates CAD points at their vertices.

The resulting points are snap references, useful for scaling the sheet from its blue registration marks. Raster-only PDF content cannot be detected.

## Enclose existing text

1. Select **xfTools → Enclose text**, or enter `TFRAME`.
2. Choose a rectangle, circle, or slot; choose **Fit** to size the shape from the text plus offset, or **Fixed** to enter dimensions.
3. Select `TEXT` entities to enclose; press **Enter** or **Esc** to finish.

On Linux, use `TFRAME:rectangle,fit,1,10,5` (shape, mode, offset, width, height/diameter).

## Configure or restart numbering

Every `PNUM` launch opens a compact settings window with:

- start number;
- increment (which can be negative);
- prefix;
- text height;
- upper-right offset;
- text style.

Change values and select **Start**. Select **Cancel** to leave the command inactive. Values from the latest run remain available until OpenCADStudio is closed.

## Label appearance

- Labels are `TEXT` entities, so they can be selected, moved, edited, or deleted like standard CAD text.
- Height, offset, and style use the values selected in the settings window.

## Troubleshooting

| Problem | Check |
| --- | --- |
| The **xfTools** tab is missing | Ensure the DLL and `plugin.toml` are in the same folder, then restart OpenCADStudio. |
| The plugin does not load | Confirm you are using OpenCADStudio `v2026.38`; the plugin depends on its API. |
| Labels are too large or small | Set the required height in the window before selecting **Start**. |
| The number does not restart | Set the required start number in the window before selecting **Start**. |
