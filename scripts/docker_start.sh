#!/usr/bin/env bash
set -euo pipefail

# This script starts the rust_comms CLI with the expected arguments, taking
# configuration from environment variables.
# Required env vars:
#   PASSPHRASE  - passphrase to unlock identity
#   PORT        - UDP/TCP port to bind/expose
#   HANDLE      - User handle
# Optional env vars:
#   RELAY       - set to enable relay mode with --relay and --static-ip params
#   EXTERNAL_IP - externally reachable IP or DNS name; if blank, auto-detected (only used when RELAY is set)
#   EXTRA_ARGS  - any extra args to pass to the CLI
#   STUN_FILE   - path to STUN servers file (default /app/stunservers.txt)
#   NODE_FILE   - path to node configuration JSON (default /app/nodely_testnet_node.json)
#   NAT_MODE    - Direct|Full|Restricted (default Direct)

: "${PASSPHRASE:?Environment variable PASSPHRASE must be set}"
: "${PORT:?Environment variable PORT must be set}"
: "${HANDLE:?Environment variable HANDLE must be set}"

STUN_FILE=${STUN_FILE:-/app/stunservers.txt}
NODE_FILE=${NODE_FILE:-/app/nodely_testnet_node.json}
SENTINEL_FILE=${SENTINEL_FILE:-}
NAT_MODE=${NAT_MODE:-Direct}

# Configure NAT/iptables rules inside the container to emulate NAT types
configure_nat() {
  local mode="$1"
  echo "[docker_start][NAT] NAT_MODE=${mode}"
  if [[ "${mode}" == "Direct" ]]; then
    echo "[docker_start][NAT] Direct mode: no iptables changes"
    return 0
  fi

  if ! command -v iptables >/dev/null 2>&1; then
    echo "[docker_start][NAT][WARN] iptables not available; skipping NAT emulation" >&2
    return 0
  fi

  # Determine primary interface (default eth0)
  local IFACE
  IFACE=$(ip route show default 2>/dev/null | awk '/default/ {for(i=1;i<=NF;i++) if($i=="dev") {print $(i+1); exit}}')
  IFACE=${IFACE:-eth0}
  echo "[docker_start][NAT] Using interface ${IFACE}"

  # Reset rules
  iptables -F || true
  iptables -t nat -F || true
  iptables -t raw -F || true
  iptables -t mangle -F || true

  # Baseline policies
  iptables -P INPUT DROP || true
  iptables -P FORWARD DROP || true
  iptables -P OUTPUT ACCEPT || true
  iptables -A INPUT -i lo -j ACCEPT
  iptables -A INPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT

  case "${mode}" in
    Full)
      echo "[docker_start][NAT] Applying Full cone approximation"
      iptables -t nat -A POSTROUTING -o "$IFACE" -j MASQUERADE
      iptables -A INPUT -i "$IFACE" -p udp --dport 1024:65535 -j ACCEPT
      iptables -A INPUT -i "$IFACE" -p tcp --dport 1024:65535 -j ACCEPT
      ;;
    Restricted)
      echo "[docker_start][NAT] Applying Restricted cone approximation"
      iptables -t nat -A POSTROUTING -o "$IFACE" -j MASQUERADE
      iptables -A OUTPUT -o "$IFACE" -p udp -m state --state NEW -m recent --name bingle_peers --set
      iptables -A OUTPUT -o "$IFACE" -p tcp -m state --state NEW -m recent --name bingle_peers --set
      iptables -A INPUT -i "$IFACE" -p udp -m recent --name bingle_peers --rcheck -j ACCEPT
      iptables -A INPUT -i "$IFACE" -p tcp -m recent --name bingle_peers --rcheck -j ACCEPT
      ;;
    *)
      echo "[docker_start][NAT][WARN] Unknown NAT_MODE='${mode}'; leaving default rules" >&2
      ;;
  esac
}

# Discover external IP if not provided or blank (only when RELAY is set and STUN_ONLY is not set)
if [[ -n "${RELAY:-}" ]] && [[ -z "${EXTERNAL_IP:-}" ]] && [[ -z "${STUN_ONLY:-}" ]]; then
  echo "RELAY mode enabled but EXTERNAL_IP not provided; attempting autodiscovery..."
  # Preferred: use routing lookup to a public IP to find the egress interface IP
  if command -v ip >/dev/null 2>&1; then
    EXTERNAL_IP=$(ip -o -4 route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="src"){print $(i+1); exit}}') || true
    if [[ -z "${EXTERNAL_IP}" ]]; then
      # Fallback: first global IPv4 address
      EXTERNAL_IP=$(ip -o -4 addr show scope global 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | head -n1) || true
    fi
  fi
  # Fallback to ifconfig if available
  if [[ -z "${EXTERNAL_IP:-}" ]] && command -v ifconfig >/dev/null 2>&1; then
    EXTERNAL_IP=$(ifconfig 2>/dev/null | awk '/inet / && $2 !~ /127\.0\.0\.1/ {print $2; exit} /inet addr:/ {gsub("addr:","",$2); if($2!="127.0.0.1"){print $2; exit}}') || true
  fi
  # Fallback to hostname -I
  if [[ -z "${EXTERNAL_IP:-}" ]] && command -v hostname >/dev/null 2>&1; then
    EXTERNAL_IP=$(hostname -I 2>/dev/null | awk '{print $1}') || true
  fi
  if [[ -z "${EXTERNAL_IP:-}" ]]; then
    echo "Failed to autodetect EXTERNAL_IP for relay mode. Relay will use STUN discovery." >&2
  else
    echo "Autodetected EXTERNAL_IP=${EXTERNAL_IP}"
  fi
fi

if [[ ! -s "$STUN_FILE" ]]; then
  echo "STUN servers file not found or empty: $STUN_FILE" >&2
  exit 2
fi

if [[ ! -s "$NODE_FILE" ]]; then
  echo "Node configuration file not found or empty: $NODE_FILE" >&2
  exit 2
fi

# Apply NAT emulation (if any) before starting CLI
configure_nat "$NAT_MODE" || true

CMD=("/app/bingle_cli" \
  "run" \
  "--handle" "$HANDLE" \
  "--passphrase" "$PASSPHRASE"
)

# Add relay-specific parameters if RELAY flag is set
if [[ -n "${RELAY:-}" ]]; then
  if [[ -n "${EXTERNAL_IP:-}" ]]; then
    CMD+=("--relay" "--static-ip" "${EXTERNAL_IP}:${PORT}")
  else
    CMD+=("--relay")
  fi
fi

CMD+=("--stun-servers-file" "$STUN_FILE" "--node-file" "$NODE_FILE")

# Append sentinel-file argument if provided
if [[ -n "$SENTINEL_FILE" ]]; then
  CMD+=("--sentinel-file" "$SENTINEL_FILE")
fi

if [[ -n "${EXTRA_ARGS:-}" ]]; then
  # shellcheck disable=SC2206
  EXTRA_ARR=( ${EXTRA_ARGS} )
  CMD+=("${EXTRA_ARR[@]}")
fi

echo "Starting: ${CMD[*]}"
exec "${CMD[@]}"
