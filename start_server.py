#!/usr/bin/env python3
"""Start code-server in a Kaggle Linux runtime."""

import argparse
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
SERVER_LOG = WORKSPACE / "code-server.log"
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


def wait_for_code_server(process=None, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process is not None and process.poll() is not None:
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
    if is_healthy():
        raise RuntimeError(
            f"Something is already responding at {HEALTH_URL}. "
            "Stop that service before starting another one."
        )

    LOCAL_BIN.mkdir(parents=True, exist_ok=True)
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    WORKSPACE.mkdir(parents=True, exist_ok=True)

    install_code_server()

    password = secrets.token_urlsafe(24)
    CONFIG_FILE.write_text(
        f"bind-addr: 127.0.0.1:{PORT}\n"
        "auth: password\n"
        f"password: {password}\n"
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
    except BaseException:
        terminate_process(process)
        SERVER_PID_FILE.unlink(missing_ok=True)
        raise

    print("code-server is running.")
    print("Workspace:", WORKSPACE)
    return process


def print_connection_details():
    """Print the local address and saved password for code-server."""
    if not CONFIG_FILE.exists():
        raise RuntimeError(f"Password configuration was not found: {CONFIG_FILE}")

    config_text = CONFIG_FILE.read_text(encoding="utf-8")
    password_match = re.search(r"(?m)^password:\s*(\S+)\s*$", config_text)
    if password_match is None:
        raise RuntimeError("Could not read the password from the code-server config.")

    print("=" * 55)
    print("CODE-SERVER IS READY")
    print("Local URL: http://127.0.0.1:" + str(PORT))
    print("PASSWORD (keep private):", password_match.group(1))
    print("=" * 55)


def supervise_services(server_process):
    """Keep the foreground controller alive while code-server is running."""
    required_server_parts = ("--config", str(CONFIG_FILE))
    print("Foreground mode: press Ctrl+C to stop services started by this run.")

    while True:
        server_running = (
            server_process.poll() is None
            if server_process is not None
            else recorded_process_is_running(SERVER_PID_FILE, required_server_parts)
        )
        if not server_running:
            print("code-server stopped; shutting down managed services.")
            return
        time.sleep(1)


def stop_started_services(server_process):
    """Stop only services launched by this invocation and remove their state."""
    if server_process is not None:
        terminate_process(server_process)
        SERVER_PID_FILE.unlink(missing_ok=True)
        CONFIG_FILE.unlink(missing_ok=True)


def handle_termination_signal(_signum, _frame):
    """Route SIGTERM through the normal foreground cleanup path."""
    raise KeyboardInterrupt


def main(foreground=False):
    required_server_parts = ("--config", str(CONFIG_FILE))
    server_running = recorded_process_is_running(
        SERVER_PID_FILE,
        required_server_parts,
    )

    server_process = None
    previous_sigterm_handler = None
    if foreground:
        previous_sigterm_handler = signal.signal(
            signal.SIGTERM,
            handle_termination_signal,
        )

    try:
        if server_running:
            print("code-server is already running.")
            wait_for_code_server()
        else:
            server_process = start_code_server()

        print_connection_details()
        if foreground:
            supervise_services(server_process)
    except BaseException as error:
        if server_process is not None:
            terminate_process(server_process)
            SERVER_PID_FILE.unlink(missing_ok=True)
            CONFIG_FILE.unlink(missing_ok=True)
        elif not server_running:
            CONFIG_FILE.unlink(missing_ok=True)

        if isinstance(error, KeyboardInterrupt) and foreground:
            print("\nShutdown requested.")
        else:
            raise
    finally:
        if foreground:
            stop_started_services(server_process)
            signal.signal(signal.SIGTERM, previous_sigterm_handler)

if __name__ == "__main__":
    if os.name != "posix" or not Path("/proc").exists():
        sys.exit("These scripts are intended for a Linux Kaggle runtime.")

    parser = argparse.ArgumentParser(
        description="Start code-server locally in a Kaggle Linux runtime."
    )
    parser.add_argument(
        "--foreground",
        action="store_true",
        help="keep this process running and supervise the services",
    )
    args = parser.parse_args()

    try:
        main(foreground=args.foreground)
    except Exception as error:
        sys.exit(f"Failed to start services: {error}")
