#!/usr/bin/env bash
# Schneidet den Verkehr zwischen Original-App und Lüfter mit (für HoloFan Studio / src/device.rs).
# Aufruf: sudo bash capture-fan.sh   -> Original-App benutzen -> Strg+C
# Ergebnis: ~/fan-<Datum>.pcap  (am Mac in Wireshark öffnen)
OUT="$HOME/fan-$(date +%Y%m%d-%H%M%S).pcap"
echo "Schreibe nach $OUT – jetzt in der Original-App abspielen, hochladen, Helligkeit ändern …  (Strg+C beendet)"
tcpdump -i wlan0 -s 0 -w "$OUT" host 192.168.4.1
