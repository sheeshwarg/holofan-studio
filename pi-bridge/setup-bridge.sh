#!/usr/bin/env bash
# HoloFan-Bridge: Raspberry Pi (Raspberry Pi OS Bookworm, NetworkManager)
#   Heimnetz --eth0--> [Pi] --wlan0--> Lüfter-WLAN (192.168.4.0/24)
# Der Pi meldet sich per WLAN beim Lüfter an und leitet Pakete aus dem Heimnetz
# per NAT weiter. Am Lüfter wird nichts verändert.
#
# Aufruf auf dem Pi:  sudo bash setup-bridge.sh
# Rückgängig:         sudo bash setup-bridge.sh --remove
set -euo pipefail

# ---------- HIER ANPASSEN ----------
FAN_SSID="3D-Circle-XXXX"     # Name des Lüfter-WLANs
FAN_PSK=""                    # Passwort; leer lassen = wird beim Start abgefragt (offenes WLAN: "none")
LAN_IF="eth0"                 # Kabel zum Router
WIFI_IF="wlan0"               # WLAN-Modul -> Lüfter
FAN_NET="192.168.4.0/24"      # Netz des Lüfters
# -----------------------------------

CON="holofan"
NFT="/etc/holofan-nat.nft"
UNIT="/etc/systemd/system/holofan-nat.service"
SYSCTL="/etc/sysctl.d/99-holofan.conf"

[ "$(id -u)" -eq 0 ] || { echo "Bitte mit sudo starten."; exit 1; }

if [ "${1:-}" = "--remove" ]; then
  systemctl disable --now holofan-nat.service 2>/dev/null || true
  nft delete table ip holofan 2>/dev/null || true
  rm -f "$UNIT" "$NFT" "$SYSCTL"; systemctl daemon-reload
  sysctl -w net.ipv4.ip_forward=0 >/dev/null
  nmcli connection delete "$CON" 2>/dev/null || true
  echo "Bridge entfernt."; exit 0
fi

# Sicherheitsnetz: Der Pi darf nicht selbst per WLAN im Heimnetz hängen
if nmcli -t -f DEVICE,STATE device | grep -q "^$WIFI_IF:connected" && \
   ! nmcli -t -f GENERAL.CONNECTION device show "$WIFI_IF" | grep -q ":$CON$"; then
  echo "WARNUNG: $WIFI_IF ist gerade mit einem anderen WLAN verbunden."
  echo "Wenn du per WLAN per SSH verbunden bist, verlierst du gleich die Verbindung."
  echo "Erst LAN-Kabel anschließen und per eth0 verbinden. Weiter mit Enter, Abbruch mit Strg+C."
  read -r _
fi
ip -4 addr show "$LAN_IF" | grep -q "inet 192\.168\.4\." && { echo "Heimnetz ist 192.168.4.x – Konflikt mit dem Lüfter-Netz."; exit 1; }

if [ -z "$FAN_PSK" ]; then read -r -s -p "Passwort für '$FAN_SSID' (offen: none): " FAN_PSK; echo; fi

echo "==> Pakete"
command -v nft >/dev/null || { apt-get update -qq && apt-get install -y -qq nftables; }
command -v tcpdump >/dev/null || apt-get install -y -qq tcpdump || true

echo "==> WLAN-Verbindung zum Lüfter ($WIFI_IF)"
nmcli connection delete "$CON" >/dev/null 2>&1 || true
nmcli connection add type wifi ifname "$WIFI_IF" con-name "$CON" ssid "$FAN_SSID" \
  connection.autoconnect yes connection.autoconnect-retries 0 \
  802-11-wireless.powersave 2 \
  ipv4.method auto ipv4.never-default yes ipv4.ignore-auto-dns yes ipv6.method ignore >/dev/null
if [ "$FAN_PSK" != "none" ]; then
  nmcli connection modify "$CON" wifi-sec.key-mgmt wpa-psk wifi-sec.psk "$FAN_PSK"
fi
nmcli connection up "$CON" || echo "   (Verbindung noch nicht aktiv – ist der Lüfter an?)"

echo "==> IP-Forwarding"
echo "net.ipv4.ip_forward=1" > "$SYSCTL"
sysctl -q -p "$SYSCTL"

echo "==> NAT (nftables)"
cat > "$NFT" <<NFTEOF
add table ip holofan
delete table ip holofan
table ip holofan {
  chain postrouting {
    type nat hook postrouting priority srcnat; policy accept;
    oifname "$WIFI_IF" ip daddr $FAN_NET masquerade
  }
}
NFTEOF
cat > "$UNIT" <<UNITEOF
[Unit]
Description=HoloFan bridge NAT
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/sbin/nft -f $NFT
ExecStop=/usr/sbin/nft delete table ip holofan

[Install]
WantedBy=multi-user.target
UNITEOF
systemctl daemon-reload
systemctl enable --now holofan-nat.service >/dev/null
systemctl restart holofan-nat.service

PI_IP=$(ip -4 -o addr show "$LAN_IF" | awk '{print $4}' | cut -d/ -f1 | head -1)
echo
echo "Fertig."
echo "  Pi im Heimnetz:   ${PI_IP:-?}  (im Router fest vergeben/reservieren!)"
echo "  Pi im Lüfter-Netz: $(ip -4 -o addr show "$WIFI_IF" | awk '{print $4}' | head -1)"
ping -c 2 -W 2 192.168.4.1 >/dev/null 2>&1 && echo "  Lüfter 192.168.4.1: erreichbar ✔" || echo "  Lüfter 192.168.4.1: (noch) nicht erreichbar"
echo
echo "Nächster Schritt – statische Route im Router:"
echo "  Netz 192.168.4.0, Maske 255.255.255.0, Gateway ${PI_IP:-<Pi-IP>}"
echo "Zum Testen nur am Mac (ohne Router):  sudo route -n add 192.168.4.0/24 ${PI_IP:-<Pi-IP>}"
