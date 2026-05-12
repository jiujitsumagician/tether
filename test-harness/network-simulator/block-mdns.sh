#!/usr/bin/env bash
# Simulate a network that filters mDNS by dropping all UDP/5353
# traffic. Use to verify phase 2 (UDP broadcast) engages when phase 1
# can't find anything.
#
# Usage:
#   sudo ./block-mdns.sh on    # install the rules
#   sudo ./block-mdns.sh off   # remove them

set -euo pipefail

ACTION="${1:-on}"
RULE_OUT="OUTPUT -p udp --dport 5353 -j DROP"
RULE_IN="INPUT -p udp --dport 5353 -j DROP"

apply() {
    iptables -C $RULE_OUT 2>/dev/null || iptables -A $RULE_OUT
    iptables -C $RULE_IN  2>/dev/null || iptables -A $RULE_IN
    echo "mDNS dropped on UDP/5353 (in + out)."
}

remove() {
    iptables -D $RULE_OUT 2>/dev/null || true
    iptables -D $RULE_IN  2>/dev/null || true
    echo "mDNS unblocked."
}

case "$ACTION" in
    on)  apply  ;;
    off) remove ;;
    *)   echo "usage: $0 on|off"; exit 2 ;;
esac
