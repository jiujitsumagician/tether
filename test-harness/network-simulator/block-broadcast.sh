#!/usr/bin/env bash
# Drop all packets addressed to the broadcast address so phase 2
# fails and the cascade has to fall through to the subnet probe.
#
# Usage:
#   sudo ./block-broadcast.sh on
#   sudo ./block-broadcast.sh off

set -euo pipefail

ACTION="${1:-on}"
RULE_OUT="OUTPUT -d 255.255.255.255 -j DROP"
RULE_IN="INPUT -d 255.255.255.255 -j DROP"

apply() {
    iptables -C $RULE_OUT 2>/dev/null || iptables -A $RULE_OUT
    iptables -C $RULE_IN  2>/dev/null || iptables -A $RULE_IN
    echo "Directed broadcast dropped."
}

remove() {
    iptables -D $RULE_OUT 2>/dev/null || true
    iptables -D $RULE_IN  2>/dev/null || true
    echo "Broadcast unblocked."
}

case "$ACTION" in
    on)  apply  ;;
    off) remove ;;
    *)   echo "usage: $0 on|off"; exit 2 ;;
esac
