#!/usr/bin/env python3
"""Start code-server and a Cloudflare Quick Tunnel in a Kaggle Linux runtime."""

import argparse
import os
import platform
import re
import secrets
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from urllib.request import urlopen

PORT = int(os.environ.get("CODE_SERVER_PORT", "8001"))
WORKSPACE = Path(
    os.environ.get(
        "WORKSPACE_DIR",
        os.environ.get("KAGGLE_WORKING_DIR", "/workspaces"),
    )
).resolve()
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
LOG_DIR = WORKSPACE / "log"
LOG_DIR.mkdir(parents=True, exist_ok=True)
SERVER_LOG = LOG_DIR / "code-server.log"
TUNNEL_LOG = LOG_DIR / "cloudflared.log"
CLOUDFLARED_URL = (
    "https://github.com/cloudflare/cloudflared/releases/latest/download/"
    "cloudflared-linux-amd64"
)
HEALTH_URL = f"http://127.0.0.1:{PORT}/healthz"
SERVICES_DIR = Path(__file__).resolve().parent / "services"

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


def copy_services_to_workspace():
    """Copy the services directory into the configured workspace."""
    source_services = SERVICES_DIR.resolve()
    dest_services = (WORKSPACE / "services").resolve()

    if not source_services.is_dir():
        return

    if source_services == dest_services:
        return

    print(f"Copying services from {source_services} to {dest_services}...")
    try:
        shutil.copytree(source_services, dest_services, dirs_exist_ok=True)
        print("Services copied successfully.")
    except Exception as error:
        print(f"Warning: Failed to copy services to {dest_services}: {error}")


def ensure_workspace_compatibility():
    """Create a symlink for /workspace <-> /workspaces if running on Linux."""
    if os.name != "posix":
        return

    try:
        alt_workspace = Path("/workspace")
        if WORKSPACE != alt_workspace and not alt_workspace.exists():
            alt_workspace.symlink_to(WORKSPACE)
    except Exception:
        pass


def run_install_extensions():
    """Run services/coder/install_extension.sh to install extensions."""
    script_candidates = [
        WORKSPACE / "services" / "coder" / "install_extension.sh",
        SERVICES_DIR / "coder" / "install_extension.sh",
        Path(__file__).resolve().parent / "install_extension.sh",
    ]
    script_path = next((p for p in script_candidates if p.is_file()), None)
    if not script_path:
        print("Warning: Extension script not found at services/coder/install_extension.sh")
        return

    print(f"Installing extensions using {script_path}...")
    try:
        subprocess.run(
            ["bash", str(script_path)],
            check=True,
            env=os.environ.copy(),
        )
    except subprocess.CalledProcessError as error:
        print(f"Warning: Extension installation failed (exit code {error.returncode}).")
    except Exception as error:
        print(f"Warning: Could not run extension script: {error}")


def get_cloudflared_download_url():
    """Determine the cloudflared release asset URL based on machine architecture."""
    arch = platform.machine().lower()
    if arch in ("aarch64", "arm64"):
        return (
            "https://github.com/cloudflare/cloudflared/releases/latest/download/"
            "cloudflared-linux-arm64"
        )
    return CLOUDFLARED_URL


def install_cloudflared():
    """Download cloudflared into the user's local bin directory when needed."""
    if find_executable("cloudflared"):
        return

    LOCAL_BIN.mkdir(parents=True, exist_ok=True)
    target = LOCAL_BIN / "cloudflared"
    download_url = get_cloudflared_download_url()
    print("Downloading cloudflared...")
    with urlopen(download_url, timeout=60) as response:
        target.write_bytes(response.read())
    target.chmod(0o755)

    if not find_executable("cloudflared"):
        raise RuntimeError("cloudflared was downloaded, but the executable was not found.")


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


def start_code_server(install_extensions=False):
    if is_healthy():
        raise RuntimeError(
            f"Something is already responding at {HEALTH_URL}. "
            "Stop that service before starting another one."
        )

    LOCAL_BIN.mkdir(parents=True, exist_ok=True)
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    WORKSPACE.mkdir(parents=True, exist_ok=True)
    ensure_workspace_compatibility()
    copy_services_to_workspace()

    install_code_server()
    if install_extensions:
        run_install_extensions()

    password = secrets.token_urlsafe(32)
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


def read_tunnel_url():
    """Return the public Quick Tunnel URL from the tunnel log."""
    if not TUNNEL_LOG.exists():
        return None

    log_text = TUNNEL_LOG.read_text(encoding="utf-8", errors="ignore")
    match = re.search(r"https://[a-zA-Z0-9-]+\.trycloudflare\.com", log_text)
    return match.group(0) if match else None


def wait_for_tunnel_url(process=None, timeout=60):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process is not None and process.poll() is not None:
            raise RuntimeError("cloudflared exited before publishing its URL.")
        public_url = read_tunnel_url()
        if public_url:
            return public_url
        time.sleep(1)
    raise TimeoutError("Timed out waiting for a Cloudflare Quick Tunnel URL.")


def start_tunnel(token=None):
    cloudflared = find_executable("cloudflared")
    if cloudflared is None:
        raise RuntimeError("cloudflared was not found after installation.")

    tunnel_log = TUNNEL_LOG.open("w", encoding="utf-8")
    try:
        cmd = [str(cloudflared), "tunnel"]
        if token:
            cmd.extend(["run", "--token", token, "--no-autoupdate"])
        else:
            cmd.extend(["--url", f"http://127.0.0.1:{PORT}", "--no-autoupdate"])

        process = subprocess.Popen(
            cmd,
            stdin=subprocess.DEVNULL,
            stdout=tunnel_log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
    finally:
        tunnel_log.close()

    TUNNEL_PID_FILE.write_text(f"{process.pid}\n", encoding="utf-8")
    try:
        if not token:
            wait_for_tunnel_url(process)
        else:
            time.sleep(3)
            if process.poll() is not None:
                raise RuntimeError(
                    f"cloudflared exited unexpectedly with code {process.returncode}."
                )
    except BaseException:
        terminate_process(process)
        TUNNEL_PID_FILE.unlink(missing_ok=True)
        raise

    print("Cloudflare Tunnel is running.")
    return process


def print_connection_details(tunnel_enabled=True, tunnel_token=None):
    """Print the saved password and connection details for the running services."""
    if not CONFIG_FILE.exists():
        raise RuntimeError(f"Password configuration was not found: {CONFIG_FILE}")

    config_text = CONFIG_FILE.read_text(encoding="utf-8")
    password_match = re.search(r"(?m)^password:\s*(\S+)\s*$", config_text)
    if password_match is None:
        raise RuntimeError("Could not read the password from the code-server config.")

    print("=" * 55)
    print("CODE-SERVER IS READY")
    if tunnel_enabled:
        if tunnel_token:
            print("TUNNEL: Cloudflare Named Tunnel is active")
        else:
            public_url = read_tunnel_url()
            if public_url is None:
                raise RuntimeError(f"Could not find the tunnel URL in {TUNNEL_LOG}.")
            print("URL:", public_url)
    print("Local URL: http://127.0.0.1:" + str(PORT))
    print("PASSWORD (keep private):", password_match.group(1))
    print("=" * 55)


def supervise_services(server_process, tunnel_process):
    """Keep the foreground controller alive while services are running."""
    required_server_parts = ("--config", str(CONFIG_FILE))
    required_tunnel_parts = ("cloudflared", "tunnel")
    print("Foreground mode: press Ctrl+C to stop services started by this run.")

    while True:
        server_running = (
            server_process.poll() is None
            if server_process is not None
            else recorded_process_is_running(SERVER_PID_FILE, required_server_parts)
        )
        tunnel_running = (
            (
                tunnel_process.poll() is None
                if tunnel_process is not None
                else recorded_process_is_running(TUNNEL_PID_FILE, required_tunnel_parts)
            )
            if (tunnel_process is not None or TUNNEL_PID_FILE.exists())
            else True
        )
        if not server_running or not tunnel_running:
            stopped = []
            if not server_running:
                stopped.append("code-server")
            if not tunnel_running:
                stopped.append("Cloudflare Tunnel")
            print(f"{', '.join(stopped)} stopped; shutting down managed services.")
            return
        time.sleep(1)


def stop_started_services(server_process, tunnel_process):
    """Stop only services launched by this invocation and remove their state."""
    if tunnel_process is not None:
        terminate_process(tunnel_process)
        TUNNEL_PID_FILE.unlink(missing_ok=True)
    if server_process is not None:
        terminate_process(server_process)
        SERVER_PID_FILE.unlink(missing_ok=True)
        CONFIG_FILE.unlink(missing_ok=True)


def handle_termination_signal(_signum, _frame):
    """Route SIGTERM through the normal foreground cleanup path."""
    raise KeyboardInterrupt


def main(
    foreground=False,
    no_tunnel=False,
    tunnel_token=None,
    workspace=None,
    port=None,
    install_extensions=False,
):
    global PORT, HEALTH_URL, WORKSPACE, SERVER_LOG, TUNNEL_LOG
    if port:
        PORT = port
        HEALTH_URL = f"http://127.0.0.1:{PORT}/healthz"
    if workspace:
        WORKSPACE = Path(workspace).resolve()
        SERVER_LOG = WORKSPACE / "code-server.log"
        TUNNEL_LOG = WORKSPACE / "cloudflared.log"

    WORKSPACE.mkdir(parents=True, exist_ok=True)
    ensure_workspace_compatibility()
    copy_services_to_workspace()
    tunnel_enabled = not no_tunnel
    required_server_parts = ("--config", str(CONFIG_FILE))
    required_tunnel_parts = ("cloudflared", "tunnel")

    server_running = recorded_process_is_running(
        SERVER_PID_FILE,
        required_server_parts,
    )
    tunnel_running = (
        recorded_process_is_running(TUNNEL_PID_FILE, required_tunnel_parts)
        if tunnel_enabled
        else False
    )

    server_process = None
    tunnel_process = None
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
            if install_extensions:
                run_install_extensions()
        else:
            server_process = start_code_server(install_extensions=install_extensions)

        if tunnel_enabled:
            install_cloudflared()
            if tunnel_running:
                print("Cloudflare Tunnel is already running.")
                if not tunnel_token:
                    wait_for_tunnel_url(timeout=30)
            else:
                tunnel_process = start_tunnel(token=tunnel_token)

        print_connection_details(tunnel_enabled=tunnel_enabled, tunnel_token=tunnel_token)
        if foreground:
            supervise_services(server_process, tunnel_process)
    except BaseException as error:
        if tunnel_process is not None:
            terminate_process(tunnel_process)
            TUNNEL_PID_FILE.unlink(missing_ok=True)
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
            stop_started_services(server_process, tunnel_process)
            signal.signal(signal.SIGTERM, previous_sigterm_handler)


if __name__ == "__main__":
    if os.name != "posix" or not Path("/proc").exists():
        sys.exit("These scripts are intended for a Linux Kaggle runtime.")

    parser = argparse.ArgumentParser(
        description="Start code-server and expose it via Cloudflare Tunnel in a Kaggle Linux runtime."
    )
    parser.add_argument(
        "--foreground",
        action="store_true",
        help="keep this process running and supervise the services",
    )
    parser.add_argument(
        "--no-tunnel",
        action="store_true",
        help="do not start Cloudflare Tunnel (local only)",
    )
    parser.add_argument(
        "--tunnel-token",
        default=os.environ.get("CLOUDFLARE_TUNNEL_TOKEN"),
        help="Cloudflare named tunnel token (optional)",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=PORT,
        help="port used by code-server (default: 8001)",
    )
    parser.add_argument(
        "--workspace",
        default=None,
        help="workspace directory opened in code-server (default: /workspaces)",
    )
    parser.add_argument(
        "--install-extensions",
        "--install-extension",
        "--extensions",
        dest="install_extensions",
        action="store_true",
        default=os.environ.get("INSTALL_EXTENSIONS", "").lower() in ("1", "true", "yes"),
        help="install VS Code extensions from services/coder/install_extension.sh during startup",
    )
    args = parser.parse_args()

    try:
        main(
            foreground=args.foreground,
            no_tunnel=args.no_tunnel,
            tunnel_token=args.tunnel_token,
            workspace=args.workspace,
            port=args.port,
            install_extensions=args.install_extensions,
        )
    except Exception as error:
        sys.exit(f"Failed to start services: {error}")
