#!/usr/bin/env python3
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
helper = root / 'scripts/package-native-stage.py'
with tempfile.TemporaryDirectory() as temporary:
    stage = Path(temporary) / 'stage'
    for sub, names in [('bin', ['oasis7_game_launcher', 'oasis7_web_launcher',
        'oasis7_viewer_live', 'oasis7_chain_runtime', 'oasis7_client_launcher']),
        ('ops/bin', ['oasis7_world_repair_rebuild', 'oasis7_governance_registry_import',
        'oasis7_governance_registry_audit', 'service-readback'])]:
        (stage / sub).mkdir(parents=True)
        for name in names:
            (stage / sub / name).write_bytes(b'fixture binary')
            (stage / sub / name).chmod(0o755)
    args = [str(stage), str(root), 'packaging', 'native', 'linux-x64', '1']
    def invoke(mode, arguments=args, succeeds=True):
        result = subprocess.run(['python3', str(helper), mode, *arguments], capture_output=True)
        assert (result.returncode == 0) == succeeds, result.stderr.decode()
    invoke('write')
    invoke('verify')
    for index, replacement in [(2, 'dev'), (3, 'other-target'), (4, 'windows-x64'), (5, '0')]:
        changed = args.copy()
        changed[index] = replacement
        invoke('verify', changed, False)
    receipt = stage / 'native-build.json'
    original = receipt.read_text()
    receipt.write_text(original.replace(subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip(), '0' * 40))
    invoke('verify', succeeds=False)
    receipt.write_text(original)
    binary = stage / 'bin/oasis7_viewer_live'
    binary.write_bytes(b'tampered')
    invoke('verify', succeeds=False)
    binary.write_bytes(b'fixture binary')
    binary.chmod(0o644)
    invoke('verify', succeeds=False)
    binary.chmod(0o755)
    binary.unlink()
    invoke('verify', succeeds=False)
    other = stage / 'other'
    other.write_bytes(b'fixture binary')
    binary.symlink_to(other)
    invoke('verify', succeeds=False)
print('ok: native stage rejects wrong source/profile/target/platform/ops, tamper, mode, missing and symlink')
