#!/usr/bin/env bash
# Host-side networking for the `net` rows of hil-tests.conf: the LAN address
# the device dials back to and the TCP/UDP listeners it talks to.
#
# Sourced by lib.sh (which sets REPO_ROOT and SCRIPT_DIR); do not run or
# source directly. Functions only -- no work happens at source time.

# ── Host-side networking for `net` rows (hil-run.sh, sim-run.sh) ───────────
#
# The `net` rows in hil-tests.conf (netdemo, http_get) need two servers on the
# test host: a TCP echo on port 7000 and an HTTP server on port 8000. The
# runners start them just before the first `net` row and stop them on exit.

NET_ECHO_PORT=7000
NET_HTTP_PORT=8000
NET_LISTENER_PIDS=()

# Prints this machine's LAN IPv4 address (the one a board on the same network
# reaches). Prints nothing on failure. The address is DHCP-assigned, so it is
# looked up on every run rather than hardcoded.
host_lan_ip() {
  ip -4 route get 1.1.1.1 2>/dev/null | grep -oP 'src \K[\d.]+' | head -1
}

# True when something accepts TCP connections on 127.0.0.1:<port>.
net_port_open() {
  local port="$1"
  (exec 3<> "/dev/tcp/127.0.0.1/$port") 2>/dev/null
}

# Wait up to <secs> for a port to accept connections. Returns 1 on timeout.
net_wait_port() {
  local port="$1" secs="${2:-5}" i=0
  while (( i < secs * 10 )); do
    net_port_open "$port" && return 0
    sleep 0.1
    i=$((i + 1))
  done
  return 1
}

# Start the echo and HTTP servers in the background.
# Args: log_dir — where net-echo.log / net-http.log go.
# Records the PIDs in NET_LISTENER_PIDS; a second call is a no-op. Returns 1
# with the reason in NET_LISTENER_ERR when socat/python3 are missing, a port
# is already taken by someone else, or a server does not come up within 5 s.
# Call it directly, never inside `$(...)`: a command substitution runs in a
# subshell, so the PIDs would be lost and the servers would leak.
NET_LISTENER_ERR=""
start_net_listeners() {
  local log_dir="$1"
  NET_LISTENER_ERR=""
  [[ ${#NET_LISTENER_PIDS[@]} -gt 0 ]] && return 0

  # hil-fleet.sh runs the two servers once for all its runners (the ports
  # are host-global); a runner then only checks they are up and stops
  # nothing (NET_LISTENER_PIDS stays empty).
  if [[ "${PICODROID_NET_LISTENERS_EXTERNAL:-0}" == "1" ]]; then
    local port
    for port in "$NET_ECHO_PORT" "$NET_HTTP_PORT"; do
      if ! net_wait_port "$port" 5; then
        NET_LISTENER_ERR="net listeners: PICODROID_NET_LISTENERS_EXTERNAL=1 but nothing listens on port $port"
        return 1
      fi
    done
    return 0
  fi

  # hil-fleet.sh runs the two servers once for all its runners (the ports
  # are host-global); a runner then only checks they are up and stops
  # nothing (NET_LISTENER_PIDS stays empty).
  if [[ "${PICODROID_NET_LISTENERS_EXTERNAL:-0}" == "1" ]]; then
    local port
    for port in "$NET_ECHO_PORT" "$NET_HTTP_PORT"; do
      if ! net_wait_port "$port" 5; then
        NET_LISTENER_ERR="net listeners: PICODROID_NET_LISTENERS_EXTERNAL=1 but nothing listens on port $port"
        return 1
      fi
    done
    return 0
  fi

  local tool
  for tool in socat python3; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      NET_LISTENER_ERR="net listeners: '$tool' not installed"
      return 1
    fi
  done
  local port
  for port in "$NET_ECHO_PORT" "$NET_HTTP_PORT"; do
    if net_port_open "$port"; then
      NET_LISTENER_ERR="net listeners: port $port is already in use"
      return 1
    fi
  done

  local www
  www="$(mktemp -d)"
  # setsid: each server gets its own process group so stop_net_listeners can
  # kill the group (socat forks a child per connection) without touching us.
  setsid socat "TCP-LISTEN:${NET_ECHO_PORT},fork,reuseaddr" EXEC:cat \
    > "$log_dir/net-echo.log" 2>&1 < /dev/null &
  NET_LISTENER_PIDS+=($!)
  setsid python3 -m http.server "$NET_HTTP_PORT" --bind 0.0.0.0 --directory "$www" \
    > "$log_dir/net-http.log" 2>&1 < /dev/null &
  NET_LISTENER_PIDS+=($!)

  for port in "$NET_ECHO_PORT" "$NET_HTTP_PORT"; do
    if ! net_wait_port "$port" 5; then
      NET_LISTENER_ERR="net listeners: port $port did not come up (see $log_dir/net-*.log)"
      stop_net_listeners
      return 1
    fi
  done
  return 0
}

# Stop the servers started by start_net_listeners. Kills by PID only — never
# by name pattern: `pkill -f socat` would also match the shell that launched
# this script if its command line mentions the word.
stop_net_listeners() {
  local pid
  for pid in ${NET_LISTENER_PIDS[@]+"${NET_LISTENER_PIDS[@]}"}; do
    kill -TERM -- "-$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
  done
  for pid in ${NET_LISTENER_PIDS[@]+"${NET_LISTENER_PIDS[@]}"}; do
    wait "$pid" 2>/dev/null || true
  done
  NET_LISTENER_PIDS=()
}
