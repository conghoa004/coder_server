#!/usr/bin/env python3
"""Start code-server and a Cloudflare Quick Tunnel in a Kaggle Linux runtime."""

import os
import re
import secrets
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from urllib.request import urlopen

PORT = int(os.environ.get("CODE_SERVER_PORT", "8080"))
WORKSPACE = Path(os.environ.get("KAGGLE_WORKING_DIR", "/kaggle/working"))
LOCAL_BIN = Path.home() / ".local" / "bin"
STATE_DIR = Path(
    os.environ.get(
        "CODE_SERVER_STATE_DIR",
        str(Path.home() / ".local" / "state" / "kaggle-code-server"),
    )
)
CONFIG_FILE = STATE_DIR / "config.yaml"
SERVER_PID_FILE = STATE_DIR / "code-server.pid"
TUNNEL_PID_FILE = STATE_DIR / "cloudflared.pid"
SERVER_LOG = WORKSPACE / "code-server.log"
TUNNEL_LOG = WORKSPACE / "cloudflared.log"
CLOUDFLARED_URL = (
    "https://github.com/cloudflare/cloudflared/releases/latest/download/"
    "cloudflared-linux-amd64"
)
HEALTH_URL = f"http://127.0.0.1:{PORT}/healthz"

os.environ["PATH"] = f"{LOCAL_BIN}{os.pathsep}{os.environ.get('PATH', '')}"


def find_executable(name):
    """Find a program on PATH or in the local install directory."""
    found = shutil.which(name)
    if found:
        return Path(found)

    local_path = LOCAL_BIN / name
    return local_path if local_path.is_file() else None


def install_code_server():
    """Install code-server with its official installer when needed."""
    if find_executable("code-server"):
        return

    if shutil.which("bash") is None or shutil.which("curl") is None:
        raise RuntimeError("Installing code-server requires bash and curl.")

    print("Installing code-server...")
    subprocess.run(
        [
            "bash",
            "-o",
            "pipefail",
            "-c",
            "curl -fsSL https://code-server.dev/install.sh | "
            "sh -s -- --method=standalone",
        ],
        check=True,
    )
    os.environ["PATH"] = f"{LOCAL_BIN}{os.pathsep}{os.environ.get('PATH', '')}"

    if not find_executable("code-server"):
        raise RuntimeError("The code-server installer completed, but the executable was not found.")


def install_cloudflared():
    """Download cloudflared into the user's local bin directory when needed."""
    if find_executable("cloudflared"):
        return

    LOCAL_BIN.mkdir(parents=True, exist_ok=True)
    target = LOCAL_BIN / "cloudflared"
    print("Downloading cloudflared...")
    with urlopen(CLOUDFLARED_URL, timeout=30) as response:
        target.write_bytes(response.read())
    target.chmod(0o755)


def read_command_line(pid):
    """Return a process command line from procfs, or an empty string."""
    try:
        raw = Path(f"/proc/{pid}/cmdline").read_bytes()
    except (FileNotFoundError, PermissionError, ProcessLookupError):
        return ""
    return raw.replace(b"\0", b" ").decode("utf-8", errors="replace")


def recorded_process_is_running(pid_file, required_parts):
    """Check a saved PID and its command to avoid acting on a reused PID."""
    if not pid_file.exists():
        return False

    try:
        pid = int(pid_file.read_text(encoding="utf-8").strip())
    except (ValueError, OSError):
        pid_file.unlink(missing_ok=True)
        return False

    command_line = read_command_line(pid)
    if not command_line or not all(part in command_line for part in required_parts):
        pid_file.unlink(missing_ok=True)
        return False

    return True


def is_healthy():
    """Check whether code-server responds on its local health endpoint."""
    try:
        with urlopen(HEALTH_URL, timeout=2) as response:
            return response.status == 200
    except Exception:
        return False


def wait_for_code_server(process, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("code-server exited during startup.")
        if is_healthy():
            return
        time.sleep(1)
    raise TimeoutError(f"code-server did not become ready at {HEALTH_URL}.")


def terminate_process(process):
    """Stop a process group created by this script."""
    if process is None or process.poll() is not None:
        return

    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return

    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=5)


def start_code_server():
    global PASSWORD

    if is_healthy():
        raise RuntimeError(
            f"Something is already responding at {HEALTH_URL}. "
            "Stop that service before starting another one."
        )

    LOCAL_BIN.mkdir(parents=True, exist_ok=True)
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    WORKSPACE.mkdir(parents=True, exist_ok=True)

    install_code_server()
    install_cloudflared()

    PASSWORD = secrets.token_urlsafe(24)
    CONFIG_FILE.write_text(
        f"bind-addr: 127.0.0.1:{PORT}\n"
        "auth: password\n"
        f"password: {PASSWORD}\n"
        "cert: false\n",
        encoding="utf-8",
    )
    CONFIG_FILE.chmod(0o600)

    server_executable = find_executable("code-server")
    server_log = SERVER_LOG.open("w", encoding="utf-8")
    try:
        process = subprocess.Popen(
            [
                str(server_executable),
                "--config",
                str(CONFIG_FILE),
                "--disable-telemetry",
                str(WORKSPACE),
            ],
            stdin=subprocess.DEVNULL,
            stdout=server_log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
    finally:
        server_log.close()

    SERVER_PID_FILE.write_text(f"{process.pid}\n", encoding="utf-8")
    try:
        wait_for_code_server(process)
    except Exception:
        terminate_process(process)
        SERVER_PID_FILE.unlink(missing_ok=True)
        raise

    print("code-server is running.")
    print("Workspace:", WORKSPACE)
    print("Password (keep private):", PASSWORD)
    return process


def start_tunnel():
    cloudflared = find_executable("cloudflared")
    if cloudflared is None:
        raise RuntimeError("cloudflared was not found after installation.")

    tunnel_log = TUNNEL_LOG.open("w", encoding="utf-8")
    try:
        process = subprocess.Popen(
            [
                str(cloudflared),
                "tunnel",
                "--url",
                f"http://127.0.0.1:{PORT}",
                "--no-autoupdate",
            ],
            stdin=subprocess.DEVNULL,
            stdout=tunnel_log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
    finally:
        tunnel_log.close()

    TUNNEL_PID_FILE.write_text(f"{process.pid}\n", encoding="utf-8")
    deadline = time.monotonic() + 60
    try:
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError("cloudflared exited during startup.")

            log_text = TUNNEL_LOG.read_text(encoding="utf-8", errors="ignore")
            match = re.search(r"https://[a-zA-Z0-9-]+\.trycloudflare\.com", log_text)
            if match:
                print("Cloudflare Tunnel is ready.")
                print("URL:", match.group(0))
                return

            time.sleep(1)

        raise TimeoutError("Timed out waiting for a Cloudflare Quick Tunnel URL.")
    except Exception:
        terminate_process(process)
        TUNNEL_PID_FILE.unlink(missing_ok=True)
        raise


def main():
    required_server_parts = ("--config", str(CONFIG_FILE))
    required_tunnel_parts = ("tunnel", "--url", f"http://127.0.0.1:{PORT}")

    if recorded_process_is_running(SERVER_PID_FILE, required_server_parts):
        raise RuntimeError("A code-server started by this script is already running.")
    if recorded_process_is_running(TUNNEL_PID_FILE, required_tunnel_parts):
        raise RuntimeError("A Cloudflare Tunnel started by this script is already running.")

    server_process = None
    try:
        server_process = start_code_server()
        start_tunnel()
    except Exception:
        if TUNNEL_PID_FILE.exists():
            stop_managed_process(
                TUNNEL_PID_FILE,
                ("tunnel", "--url", f"http://127.0.0.1:{PORT}"),
            )
        terminate_process(server_process)
        SERVER_PID_FILE.unlink(missing_ok=True)
        CONFIG_FILE.unlink(missing_ok=True)
        raise


def stop_managed_process(pid_file, required_parts):
    """Stop a managed process by PID after validating its command line."""
    if not pid_file.exists():
        return

    try:
        pid = int(pid_file.read_text(encoding="utf-8").strip())
    except (ValueError, OSError):
        pid_file.unlink(missing_ok=True)
        return

    command_line = read_command_line(pid)
    if not command_line:
        pid_file.unlink(missing_ok=True)
        return
    if not all(part in command_line for part in required_parts):
        print(f"Leaving PID {pid} untouched because its command does not match.")
        return

    try:
        if os.getpgid(pid) == pid:
            os.killpg(pid, signal.SIGTERM)
        else:
            os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        pid_file.unlink(missing_ok=True)
        return

    deadline = time.monotonic() + 10
    while time.monotonic() < deadline and read_command_line(pid):
        time.sleep(0.25)

    if read_command_line(pid):
        try:
            if os.getpgid(pid) == pid:
                os.killpg(pid, signal.SIGKILL)
            else:
                os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass

    pid_file.unlink(missing_ok=True)


if __name__ == "__main__":
    if os.name != "posix" or not Path("/proc").exists():
        sys.exit("These scripts are intended for a Linux Kaggle runtime.")
    try:
        main()
    except Exception as error:
        sys.exit(f"Failed to start services: {error}")
