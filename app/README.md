# HoloFan Studio 13.0

Control and program 3D hologram fans on **Windows and macOS** (Rust + egui), in English
and with a modern look.

- **Player**: previous / single loop / play-pause / list loop / next, power, brightness
  (the fan's three levels), image alignment (CW / CCW), picture play time (5–30 s), what the
  fan plays now, device status and activity log.
- **Library**: every `.BIN` fan file in one folder with round previews; convert videos and
  pictures into `.BIN` (Colorful / Common / Highlight, zoom, live preview; needs ffmpeg),
  add `.BIN` files, upload, delete; the files stored on the fan (play / delete).
- **Playlists**: as many as you like; drag files in from the library or from Finder /
  Explorer, reorder by dragging, send to the fan (or all fans), copy to an SD card numbered
  in play order.
- **Fans**: several fans side by side with live status and quick controls, commands for all
  fans at once, and which fan the other pages control.
- **Clock**: clock on/off, white or black hands, Digital / Symbol / Zodiac / Constellation
  dial, power-on display time (h/min/s).
- **Device**: the 20 fan models (10-ND … 115-F6), connection address, Wi-Fi setup,
  clear cache, format fan storage, theme and accent colour.

**Fan protocol:** taken from the original Windows app (see `MECHANICS.md`): TCP to
192.168.4.1:20320, a hello after connecting, framed one-letter commands (power = `a`), live
status and file list from the fan, and `.BIN` uploads on a second connection. Tested against a
real 42-F2 fan: the connection stays open and the status is read.

**Video converter:** rebuilt from the original's "Decode video" (see `MECHANICS.md`).
Checked byte for byte on the 42-F2 against `.BIN` files the original made; 45-F4, 65-F4,
85-F4, 56-F4, 52-F4 and 30-F2 use the same steps but are untested; other types aren't
supported yet. Sound (.mp3) for audio fans isn't made.

**Not known yet:** the layout of `CONFIG.INI` / `CLOCK.INI` on the SD card (left untouched).

## Where is the app?

- **Windows:** `dist\HoloFan Studio.exe`, a single file that needs no installation. Rebuild it with
  `powershell -ExecutionPolicy Bypass -File tools\build-windows.ps1`.
- **macOS (.dmg):** a Mac app can only be built on a Mac. Copy this folder to the Mac and run
  `bash tools/build-macos.sh` in Terminal. It creates `dist/HoloFan Studio.app` and
  `dist/HoloFan-Studio-13.0.dmg`. Open the .dmg and drag the app onto "Applications".

## Windows

This computer uses the GNU Rust toolchain; `tools/install-rust.ps1` and
`tools/install-mingw.ps1` set it up, and `.cargo/config.toml` points at the MinGW `dlltool`.

## macOS

    bash tools/build-macos.sh

creates `dist/HoloFan Studio.app` (Apple Silicon + Intel). The first time, right-click it
and choose **Open**, because it is not notarised.

## Options

    --screenshot <file.png>   draw ~120 frames, save the window as PNG, exit
    --page <player|media|clock|device>
    --theme <system|light|dark>
    --pick-model              (with --screenshot) show the fan-model picker
