#!/usr/bin/env bash
# Simulate strict client isolation by dropping ALL traffic on
# UDP/31413 (the Tether discovery port) AND TCP/31415 (the TLS port).
# Used to verify the cascade falls through wireless entirely and
# arrives at the USB prompt.
#
# Usage:
#   sudo ./isolate-subnet.sh on
#   sudo ./isolate-subnet.sh off

set -euo pipefail

ACTION="${1:-on}"

apply() {
    iptables -C OUTPUT -p udp --dport 31413 -j DROP 2>/dev/null || iptables -A OUTPUT -p udp --dport 31413 -j DROP
    iptables -C OUTPUT -p tcp --dport 31415 -j DROP 2>/dev/null || iptables -A OUTPUT -p tcp --dport 31415 -j DROP
    iptables -C INPUT  -p udp --dport 31413 -j DROP 2>/dev/null || iptables -A INPUT  -p udp --dport 31413 -j DROP
    iptables -C INPUT  -p tcp --dport 31415 -j DROP 2>/dev/null || iptables -A INPUT  -p tcp --dport 31415 -j DROP
    echo "Subnet isolated for Tether (mDNS still passes; TLS + UDP blocked)."
}

remove() {
    iptables -D OUTPUT -p udp --dport 31413 -j DROP 2>/dev/null || true
    iptables -D OUTPUT -p tcp --dport 31415 -j DROP 2>/dev/null || true
    iptables -D INPUT  -p udp --dport 31413 -j DROP 2>/dev/null || true
    iptables -D INPUT  -p tcp --dport 31415 -j DROP 2>/dev/null || true
    echo "Subnet isolation lifted."
}

case "$ACTION" in
    on)  apply  ;;
    off) remove ;;
    *)   echo "usage: $0 on|off"; exit 2 ;;
esac
