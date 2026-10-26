#!/usr/bin/env python3
"""Observe existing Preview services and signed world reads for deployment health."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time
import urllib.request
from ecs_preview_update import binding, digest, load_config, module, preflight, trusted_path


def signed_sample(client, device, identity):
    result = subprocess.run([str(client), "--config", str(device), "capabilities"],
                            check=True, timeout=15, text=True, capture_output=True)
    value = json.loads(result.stdout)
    tick = value.get("logical_tick")
    if value.get("api_revision") != 1 or type(tick) is not int or tick < 0:
        raise ValueError("signed client did not return a verified canonical world tick")
    return {"timestamp": time.monotonic(), "world_tick": tick,
            "identity": identity, "signed_world": True}


def descendants(pid):
    result = {pid}
    # Read parent identity from /proc stat without splitting the process name.
    parents = {}
    for path in Path("/proc").glob("[0-9]*/stat"):
        try:
            fields = path.read_text().rsplit(")", 1)[1].split()
            parents[int(path.parent.name)] = int(fields[1])
        except (OSError, ValueError, IndexError):
            continue
    while True:
        extra = {child for child, parent in parents.items() if parent in result}
        if extra <= result:
            return result
        result |= extra


def verify(request, config, device):
    trusted_path(device)
    original = json.loads(device.read_text())
    identity = {"world_id": original["world_id"], "genesis_hash": original["genesis_digest"],
                "service_public_key": original["service_public_key"], "identity": original["agent_id"]}
    if request.get("identity") != identity or identity != config["world_identity"]:
        raise ValueError("configured signed device does not match the original world/read identity")
    if request.get("mode") == "rollback-safety":
        # We cannot infer write compatibility from process/API availability.
        return {"nonce": request["nonce"], "identity": identity, "chain_write_status": "unknown"}
    preflight(config)
    components = {k: binding(v["path"]) for k, v in config["components"].items()}
    if components != request.get("components"):
        raise ValueError("actual component bytes differ from the deployment request")
    player = Path(components["player"]["release"])
    pid = int(subprocess.check_output(["systemctl", "show", config["components"]["player"]["service"], "-p", "MainPID", "--value"], text=True, timeout=3))
    viewer_hash = digest(player / "bin/oasis7_viewer_live")
    if not any(digest(Path(f"/proc/{p}/exe")) == viewer_hash for p in descendants(pid) if Path(f"/proc/{p}/exe").exists()):
        raise ValueError("running Viewer child bytes do not match the player release")
    skills = module("package-ecs-skills")
    origin = skills.public_origin(config["skill"]["origin"])
    opener = urllib.request.build_opener(skills.NoRedirect())
    def fetch(path):
        with opener.open(origin + path, timeout=15) as response:
            return response.read()
    admission = json.loads(fetch("/api/public/player-session/admission"))
    if admission.get("ok") is not True or admission.get("deployment_mode") != "hosted_public_join" or admission.get("admission", {}).get("runtime_probe_status") != "ok":
        raise ValueError("public admission/API verification failed")
    # Verify exact public Web bytes, including versioned assets. No URL, command,
    # credentials or allowlist is accepted from the remote release manifest.
    web = Path(components["web"]["path"])
    import urllib.parse
    for path in sorted(web.rglob("*")):
        if path.is_file() and "skills" not in path.relative_to(web).parts:
            url = "/" + urllib.parse.quote(path.relative_to(web).as_posix(), safe="/")
            if fetch(url) != path.read_bytes():
                raise ValueError("public Web bytes do not match selected release")
    _, public = skills.checked_release(player, components["player"]["source_sha"])
    skills.verify(public, origin)
    client = player / "bin/oasis7_game_client"
    samples = [signed_sample(client, device, identity)]
    started = samples[0]["timestamp"]
    for offset in (20, 40, 60):
        time.sleep(max(0, started + offset - time.monotonic()))
        samples.append(signed_sample(client, device, identity))
    # Capture final bytes and process state too; a concurrent exit/change cannot
    # inherit the initial successful observations.
    preflight(config)
    if {k: binding(v["path"]) for k, v in config["components"].items()} != components:
        raise ValueError("components changed during the health observation")
    return {"nonce": request["nonce"], "identity": identity, "components": components,
            "checks": dict.fromkeys(("api", "web", "skill", "signed_world"), True),
            "samples": samples, "chain_write_status": "unknown"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--device", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(json.load(sys.stdin), load_config(args.config), args.device)))


if __name__ == "__main__":
    main()
