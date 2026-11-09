#!/usr/bin/env python3
"""Stop the code-server and Cloudflare Tunnel started by start_server.py."""

import os
import signal
import sys
import time
from pathlib import Path

PORT = int(os.environ.get("CODE_SERVER_PORT", "8080"))
STATE_DIR = Path(
    os.environ.get(
        "CODE_SERVER_STATE_DIR",
        str(Path.home() / ".local" / "state" / "kaggle-code-server"),
    )
)
CONFIG_FILE = STATE_DIR / "config.yaml"
SERVER_PID_FILE = STATE_DIR / "code-server.pid"
TUNNEL_PID_FILE = STATE_DIR / "cloudflared.pid"


def read_command_line(pid):
    """Return a process command line from procfs, or an empty string."""
    try:
        raw = Path(f"/proc/{pid}/cmdline").read_bytes()
    except (FileNotFoundError, PermissionError, ProcessLookupError):
        return ""
    return raw.replace(b"\0", b" ").decode("utf-8", errors="replace")


def process_group_exists(pid):
    try:
        os.killpg(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def stop_managed_process(name, pid_file, required_parts):
    """Stop a saved process only when its command line matches this service."""
    if not pid_file.exists():
        print(f"{name}: no PID file; nothing to stop.")
        return None

    try:
        pid = int(pid_file.read_text(encoding="utf-8").strip())
    except (ValueError, OSError):
        pid_file.unlink(missing_ok=True)
        print(f"{name}: removed an invalid PID file.")
        return True

    command_line = read_command_line(pid)
    if not command_line:
        pid_file.unlink(missing_ok=True)
        print(f"{name}: process is already stopped.")
        return True

    if not all(part in command_line for part in required_parts):
        print(f"{name}: PID {pid} does not match; leaving it untouched.")
        return False

    try:
        if os.getpgid(pid) == pid:
            os.killpg(pid, signal.SIGTERM)
        else:
            os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        pid_file.unlink(missing_ok=True)
        print(f"{name}: process is already stopped.")
        return True

    deadline = time.monotonic() + 10
    while time.monotonic() < deadline and process_group_exists(pid):
        time.sleep(0.25)

    if process_group_exists(pid):
        try:
            os.killpg(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass

    pid_file.unlink(missing_ok=True)
    print(f"{name}: stopped.")
    return True


def main():
    required_tunnel = ("tunnel", "--url", f"http://127.0.0.1:{PORT}")
    required_server = ("--config", str(CONFIG_FILE))

    stop_managed_process("Cloudflare Tunnel", TUNNEL_PID_FILE, required_tunnel)
    server_stopped = stop_managed_process(
        "code-server",
        SERVER_PID_FILE,
        required_server,
    )

    if server_stopped is True:
        CONFIG_FILE.unlink(missing_ok=True)


if __name__ == "__main__":
    if os.name != "posix" or not Path("/proc").exists():
        sys.exit("These scripts are intended for a Linux Kaggle runtime.")
    main()
