# Brief

**Question:** Fix power-off and frequent connection loss in the HoloFan rebuild by learning the original's real network protocol (address, port, command bytes, keep-alive) from its decompiled code.

**Purpose:** Person reports: power off doesn't switch the fan off; app keeps losing connection to 192.168.4.20320 (likely 192.168.4.1:20320 or similar).

**Targets:**
- ~/Documents/Holofan/电脑软件V13.0(Windows App V13.0).zip -> @Windows app(V13.0).exe
- ~/Documents/Holofan SD card files (CLOCK.INI, CONFIG.INI, *.BIN)

**Done when:** device.rs speaks the original's protocol (from decompiled code), power command works, connection stays up with the original's keep-alive; MECHANICS.md + mechanics.json written; build + screenshot pass.
