#!/usr/bin/env python3
"""Install the Preview updater without creating or replacing game/world state."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

FILES = ("ecs-preview-verify.py", "ecs-preview-update.py", "ecs_preview_update.py", "ecs_preview_release.py",
         "ecs_preview_health.py", "ecs_preview_compatibility.py", "package-ecs-skills.py")


def units(destination, config, python, nginx_dir=Path("/etc/nginx/snippets")):
    for value in (destination, config, python, nginx_dir):
        if not str(value).startswith("/") or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_./-" for c in str(value)):
            raise ValueError("systemd paths must be absolute and contain only safe path characters")
    service = f"""[Unit]
Description=Oasis7 Preview compatible release updater
Wants=network-online.target
After=network-online.target

[Service]
Type=oneshot
User=root
UMask=0077
Environment=PYTHONDONTWRITEBYTECODE=1
ExecStart={python} {destination}/ecs-preview-update.py --config {config} tick
TimeoutStartSec=900
NoNewPrivileges=true
PrivateTmp=true
ProtectHome=true
ProtectSystem=full
ReadWritePaths={nginx_dir}
"""
    timer = """[Unit]
Description=Poll Oasis7 Preview release channel

[Timer]
OnBootSec=60s
OnUnitInactiveSec=60s
AccuracySec=1s
Unit=oasis7-preview-update.service

[Install]
WantedBy=timers.target
"""
    return {"oasis7-preview-update.service": service, "oasis7-preview-update.timer": timer}


def prepare_lock_parent(lock):
    import ecs_preview_update as updater
    missing = []
    parent = lock.parent
    while not parent.exists() and not parent.is_symlink():
        missing.append(parent)
        parent = parent.parent
    updater.trusted_path(parent)
    for directory in reversed(missing):
        directory.mkdir(mode=0o700)
        updater.trusted_path(directory)
    updater.trusted_path(lock.parent)


def install(config, destination, unit_dir, python, enable=False):
    import ecs_preview_update as updater
    config, destination, unit_dir, python = map(Path, (config, destination, unit_dir, python))
    for path in (config, unit_dir, python.resolve(strict=True), python.parent, destination if destination.exists() else destination.parent):
        updater.trusted_path(path)
    if not python.is_file() or not os.access(python, os.X_OK):
        raise ValueError("configured Python interpreter is not executable")
    conf = updater.load_config(config)
    lock = Path(conf["lock_path"])
    prepare_lock_parent(lock)
    with updater.module("package-ecs-skills").maintenance_lock(lock):
        return install_locked(conf, config, destination, unit_dir, python, enable)


def install_locked(conf, config, destination, unit_dir, python, enable):
    import ecs_preview_update as updater
    # Every service and existing component must be understood before installing
    # even an inactive timer. No world initializer or unit rewrite is called.
    updater.preflight(conf)
    root = Path(__file__).resolve().parent
    digest = hashlib.sha256()
    for name in FILES:
        digest.update(name.encode() + (root / name).read_bytes())
    version = destination / digest.hexdigest()
    rendered = units(version, config, python, Path(conf["skill"]["nginx_include"]).parent)
    if any((unit_dir / name).exists() for name in rendered):
        raise ValueError("updater units already exist; stop timer and review installation before replacing")
    destination.mkdir(parents=True, exist_ok=True)
    if not version.exists():
        with tempfile.TemporaryDirectory(prefix=".install-", dir=destination) as temp:
            staged = Path(temp) / "updater"
            staged.mkdir(mode=0o755)
            for name in FILES:
                shutil.copyfile(root / name, staged / name)
                (staged / name).chmod(0o644)
            os.rename(staged, version)
    else:
        for name in FILES:
            if (version / name).read_bytes() != (root / name).read_bytes():
                raise ValueError("installed updater content mismatch")
    for name, content in rendered.items():
        with (unit_dir / name).open("x") as stream:
            stream.write(content)
        (unit_dir / name).chmod(0o644)
    subprocess.run(["systemctl", "daemon-reload"], check=True, timeout=30)
    if enable:
        subprocess.run(["systemctl", "enable", "--now", "oasis7-preview-update.timer"], check=True, timeout=30)
    return version


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--destination", type=Path, default=Path("/opt/oasis7-preview-updater"))
    parser.add_argument("--unit-dir", type=Path, default=Path("/etc/systemd/system"))
    parser.add_argument("--python", type=Path, default=Path("/opt/oasis7-preview-updater-venv/bin/python"))
    parser.add_argument("--enable", action="store_true", help="enable the timer after successful installation")
    args = parser.parse_args()
    if os.geteuid() != 0:
        parser.error("installation requires root")
    print(install(args.config, args.destination, args.unit_dir, args.python, args.enable))


if __name__ == "__main__":
    main()
