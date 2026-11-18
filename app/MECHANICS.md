# HoloFan Studio — fan protocol (from the original "@Windows app(V13.0).exe")

Read from the original's machine code (built-in decompiler and objdump; addresses are the
original's virtual addresses). Implemented in `src/protocol.rs` and `src/device.rs`.

## Connection
| What | Value | Source |
|---|---|---|
| Fan address | 192.168.4.1 | string 0x40dc7c, set in 0x4019e0 |
| Command port | 20320 (0x4F60) | 0x4019e0 (this+0x104) |
| Upload port | 20320 (separate connection) | 0x4019e0 (this+0x108) |
| Connect | non-blocking connect + select, 100 ms | 0x408920 |
| Hello after connect | `C0EEB7C9BAA3C0EEBDF9E5B7` (24 bytes) | 0x408920 → 0x40e598 |
| Keep-alive | none: the app blocks on recv; the fan sends status frames | 0x408ef0, 0x403e80 |
| Lost connection | only when recv returns ≤ 0; then reconnect at once | 0x403e80, 0x408ef0 |

The rebuild waits 1 s between tries (estimated, gentler on Wi-Fi) and uses a 3 s connect timeout.

## Command frame (0x408a80)
`"C0EEB7C9BAA3"` + `[L/323, 0x63 + (L/17)%19, 0x62 + L%17]` + payload (L bytes) + `"C0EEBDF9E5B7"`.
Fan frames use the same shape; the original checks `b0*323 + b1*17 + b2 == frame_len + 1754`.

## Commands (payloads), from the message map at 0x40e778 and the dialog resource 102
| Button | Control id | Handler | Payload |
|---|---|---|---|
| On/Off (power) | 1066 | 0x404c10 | `a` |
| Next one | 1009 | 0x404c20 | `c` |
| Last one | 1008 | 0x404c30 | `d` |
| Play/Pause | 1051 | 0x404c40 | `e` |
| Single loop | 1079 | 0x404c70 | `g` |
| List loop | 1052 | 0x404c50 | `h` |
| Brightness− | 1053 | 0x404c60 | `l` |
| Brightness+ | 1054 | 0x404c80 | `m` |
| CW adjust | 1069 | 0x404c90 | `p` |
| CCW adjust | 1070 | 0x404ca0 | `q` |
| Format Disk | 1038 | 0x404cb0 | `j` |
| Clear Cache | 1037 | 0x404cc0 | `k` |
| Picture play time (5–30 s) | 1085 | 0x409490 | `C`, seconds |
| File list "Play" (column 4) | 1023 | 0x409050 | `B`, row |
| File list "Delete" (column 5) | 1023 | 0x409050 | `A`, row |
| Wi-Fi config (ask name) | 1057 | 0x4093b0 | `r` |
| Wi-Fi apply | dialog 140 | 0x403e80 ('r' reply) | `s`, len(name), name, password (8–16 chars) |
| Clock On / Off | 1128 / 1129 | 0x409590 / 0x4095f0 | `b`,1/0,·,·,2 |
| Needle White / Black | 1130 / 1131 | 0x409650 / 0x4096b0 | `b`,1/0,·,·,3 |
| Dial Digital/Symbol/Constellation/Zodiac | 1132–1135 | 0x409710–0x409830 | `b`,0/1/2/3,·,·,4 |
| Power-on display time | 1067 | 0x409b00 | `b`, secs as u32 LE with byte 4 = 1 |
| Clock time sync (automatic) | – | 0x403e80 | `b`, secs-of-day u32 LE with byte 4 = 0 |

`·` = bytes left over from the last time command (one shared 5-byte buffer at this+0x44df).
Clock commands are refused unless the fan reports the clock flag (this+0x44d8).

## Fan status frame `i` (0x403e80)
Data = file names as `[len][bytes]…`, then 16 status bytes:
0 current file · 1 device type (21 names, table at 0x404bb4) · 2 sound version · 3 sleeping (off) ·
4 brightness (0 high, 1 medium, else low) · 5 list loop · 6 paused · 7 clock version.
On every `i` with the clock flag the original sends the time sync. A different device type
switches the selected model. `r` = Wi-Fi name reply, `v` = a seller notice (not shown).

## Upload (0x4050b0, 0x408b70, 0x404cd0)
New connection to port 20320, wait 200 ms, send `B2DDDDEDC0EEBDF9E5B7`, then
`"B2DDDDED"` + `[L/15, 0x6a + (L/3)%5, 0x63 + L%3]` + file name + `"C0EEBDF9E5B7"`.
Reply (28 bytes): `B2DDDDED`, 3 bytes summing (b8*15+b9*3+b10) to 422, status byte (0 = refused,
code in byte 12; bit 7 = also send the .mp3), 4-byte check, `C0EEBDF9E5B7`. Then the file in
245 760-byte chunks, then `B2DDDDEDC0EEBDF9E5B7`. File dialog accepts `*.BIN` (0x40e2d4).
The 4-byte check the original compares (computed in 0x408b70) is not verified by the rebuild.

## Command pacing
The fan reads one command per network packet: several commands written back to back arrive
glued together and only the first is carried out (why the clock card's three commands didn't
take). The rebuild sends one command every 350 ms (estimated; the original sends one per click).
Live test: `l` changed status byte 4 from 1 (medium) to 2 (low), `m` back to 1; the fan
answers every command with a fresh `i` status.

## .BIN video format ("Decode video", 0x405dc0, per-model settings 0x402b60)
Per frame: OpenCV `linearPolar` of the video frame around a circle (default: the largest circle
in the middle, 0x4024d5) into `rows` × `leds` pixels (rows = angles, columns = hub→rim),
BGR→RGB, a colour curve on every byte, hub thinning, then bit planes.
| Fan (type) | LEDs | angles | bits | frame bytes | fps |
|---|---|---|---|---|---|
| 42-F2 (12) | 112 | 512 | 6 | 129 024 (0x1F800) | 12 |
| 45-F4 (0), 30-F2 (8) | 128 | 1024 | 8 | 393 216 | 12 |
| 65-F4 (1), 56-F4 (5) | 144 | 1020 | 8 | 440 640 | 12 |
| 85-F4 (2) | 192 | 700 | 8 | 403 200 | 12 |
| 52-F4 (7) | 136 | 1024 | 8 | 417 792 | 12 |
- Colour curve (0x405f80), from the "decoding mode" buttons (0x401180/0x4011c0/0x401200):
  Colorful = x(x+1)²>>16, Common = x(x+1)>>8, Highlight = x(x+1)>>7, 255 above 180.
- Frame choice (0x40651a): step = (video fps − fan fps)/fan fps; a frame is skipped while the
  counter is above 1.0 (then counter −= 1), else written and counter += step.
- Hub thinning (0x4067a1): LED 0 lit every 24th angle, LED 1 every 6th, LED 2 every 3rd,
  LED 3 every 2nd, LED 4 dark every 12th.
- Packing (0x407390): angle r is written at frame_bytes − (r+1)·stride; its 8-byte chunks k
  (3·leds/8 of them) at offset 3·leds/8 − 1 − k; plane for bit b at (7−b)·3·leds/8, byte bit j =
  bit b of chunk byte j. 6-bit fans keep bits 7…2.
- 42-F2 only (0x407a77): in every 42-byte plane chunk, bytes 2…41 in fours (b0,b1,b2,b3) become
  bits of b0/b2 interleaved (high nibble, low nibble), then b1/b3.
- Checked: frames of the person's `4Tiger.BIN` decode to the tiger and re-pack to identical bytes.
- Not rebuilt: types 3, 4, 6, 9, 10, 11, 13–20 (other packing steps), sound (.mp3) for audio fans,
  dragging the circle with the mouse (the rebuild has a zoom slider; centre fixed).

## Not known yet
- The SD card's `CONFIG.INI` / `CLOCK.INI` byte layout.
- Text encoding of file names on the fan (the original is an ANSI/GBK program; names are shown as UTF-8).
