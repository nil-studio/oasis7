#!/usr/bin/env python3
"""Keep the original full-support order across the image and host UID fixture."""
import argparse
import hashlib
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import tomllib
import uuid

IMAGE = ('ghcr.io/nil-studio/oasis7/ci-rust-release-web'
         '@sha256:f47cf4a0ce572de9b871475aed4e38c35fc7e6e128dda93a7d96e50480e03c59')
ROOT = Path('/home/runner/work/oasis7/oasis7')
TEMP = Path('/home/runner/work/_temp')
MOLD = Path('/opt/oasis7-mold/3.0.0-v1')
IMAGE_HOME = Path('/oasis7-support')
FIXTURE = 'crates/oasis7_game_client/tests/fixtures/dev_test_local_linux.py'
BINARY = 'target/debug/oasis7_game_client'
DRIVER = 'scripts/ci-tests.sh'
REPOSITORY = 'nil-studio/oasis7'
SHA = re.compile(r'[0-9a-f]{40}')


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args])


def protected_context(env, source):
    if (not SHA.fullmatch(source) or
            any(env.get(key) != expected for key, expected in {
                'GITHUB_REPOSITORY': REPOSITORY,
                'GITHUB_JOB': 'release_gate_runtime_support',
                'GITHUB_ACTIONS': 'true',
                'RUNNER_ENVIRONMENT': 'github-hosted',
                'RUNNER_OS': 'Linux',
                'RUNNER_ARCH': 'X64',
            }.items())):
        return False
    ref = env.get('GITHUB_REF')
    workflow = env.get('GITHUB_WORKFLOW_REF')
    event_sha = env.get('GITHUB_SHA', '')
    if not SHA.fullmatch(event_sha) or event_sha != env.get('GITHUB_WORKFLOW_SHA'):
        return False
    if env.get('GITHUB_EVENT_NAME') == 'workflow_dispatch':
        return ref == 'refs/heads/main' and workflow in {
            f'{REPOSITORY}/.github/workflows/{name}-packages.yml@refs/heads/main'
            for name in ('release', 'mainnet')
        }
    return (env.get('GITHUB_EVENT_NAME') == 'push' and ref is not None
            and ref.startswith('refs/tags/') and event_sha == source
            and workflow == f'{REPOSITORY}/.github/workflows/release-packages.yml@{ref}')


def exact_source(root, source):
    if git(root, 'rev-parse', 'HEAD').decode().strip() != source:
        raise ValueError('runtime support checkout differs from the authorized source')
    return git(root, 'rev-parse', 'HEAD^1').decode().strip()


def supports_image(root, source):
    exact_source(root, source)
    if subprocess.run(['git', '-C', str(root), 'cat-file', '-e', f'{source}:rust-toolchain.toml'],
                      stderr=subprocess.DEVNULL).returncode:
        return False
    toolchain = tomllib.loads(git(root, 'show', f'{source}:rust-toolchain.toml').decode())['toolchain']
    driver = git(root, 'show', f'{source}:{DRIVER}')
    return (toolchain == {
        'channel': '1.96.0', 'components': ['rustfmt', 'clippy'], 'targets': ['wasm32-unknown-unknown'],
    } and b'--full-support-phase before-cross-uid|after-cross-uid' in driver)


def canonical(path):
    if path.resolve(strict=True) != path or any(c in str(path) for c in ',\r\n'):
        raise ValueError(f'unexpected runtime support mount: {path}')
    return path


def copy_native_elf(source, destination, owner):
    canonical(source)
    flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
    with os.fdopen(os.open(source, flags), 'rb') as executable:
        before = os.fstat(executable.fileno())
        if (not stat.S_ISREG(before.st_mode) or before.st_uid != owner
                or not before.st_mode & stat.S_IXUSR
                or before.st_mode & (stat.S_ISUID | stat.S_ISGID | 0o022)):
            raise ValueError('runtime support requires one owner-executable regular Linux ELF')
        header = executable.read(20)
        if (header[:7] != b'\x7fELF\x02\x01\x01' or header[16:18] not in (b'\x02\0', b'\x03\0')
                or header[18:20] != b'\x3e\0'):
            raise ValueError('runtime support requires an x86-64 executable ELF')
        executable.seek(0)
        digest = hashlib.file_digest(executable, 'sha256').hexdigest()
        executable.seek(0)
        with destination.open('xb') as staged:
            shutil.copyfileobj(executable, staged)
        after = os.fstat(executable.fileno())
        if (before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (
                after.st_size, after.st_mtime_ns, after.st_ctime_ns):
            raise ValueError('runtime support ELF changed while staging')
    with destination.open('rb') as staged:
        if hashlib.file_digest(staged, 'sha256').hexdigest() != digest:
            raise ValueError('runtime support ELF copy differs from the image build')
    destination.chmod(0o500)
    return digest


def mount(argv, path, *, target=None, readonly=False):
    argv += ['--mount', f'type=bind,source={canonical(path)},target={target or path}' + (',readonly' if readonly else '')]


def image_phase(phase, source, parent, scratch, docker_env):
    name = 'oasis7-release-runtime-support-' + uuid.uuid4().hex
    argv = ['docker', 'run', '--rm', '--init', '--pull', 'never', '--platform', 'linux/amd64',
            '--user', '1001:1001', '--name', name, '--workdir', str(ROOT)]
    mount(argv, ROOT)
    mount(argv, scratch, target=IMAGE_HOME)
    mount(argv, MOLD, readonly=True)
    linker = MOLD / 'bin/oasis7-mold-gcc'
    identity = ROOT.stat()
    values = {
        'PATH': '/opt/oasis7-node22/bin:/opt/oasis7-rust/bin:/usr/local/bin:/usr/bin:/bin',
        'HOME': str(IMAGE_HOME), 'RUNNER_TEMP': str(IMAGE_HOME), 'TMPDIR': str(IMAGE_HOME),
        'CARGO_HOME': str(IMAGE_HOME / 'cargo'), 'GITHUB_WORKSPACE': str(ROOT),
        'RUSTUP_AUTO_INSTALL': '0', 'EXPECTED_SHA': source, 'CI_VERBOSE': '1',
        'EXPECTED_WORKSPACE_ID': f'{identity.st_dev}:{identity.st_ino}',
        'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER': str(linker),
        'OASIS7_PRODUCT_DOC_BASE': parent, 'OASIS7_PRODUCT_DOC_HEAD': source,
    }
    for key in ('CI', 'GITHUB_ACTIONS', 'RUNNER_ENVIRONMENT', 'RUNNER_OS', 'RUNNER_ARCH',
                'CARGO_TERM_COLOR', 'CARGO_INCREMENTAL', 'CARGO_NET_RETRY',
                'CARGO_REGISTRIES_CRATES_IO_PROTOCOL', 'CARGO_PROFILE_DEV_DEBUG',
                'CARGO_PROFILE_TEST_DEBUG', 'RUST_TOOLCHAIN', 'OASIS7_WASM_TOOLCHAIN',
                'OASIS7_WASM_TARGET', 'OASIS7_WASM_BUILD_STD', 'OASIS7_WASM_BUILD_STD_COMPONENTS'):
        if key in os.environ:
            values[key] = os.environ[key]
    for key, value in values.items():
        argv += ['--env', f'{key}={value}']
    command = [
        *argv, IMAGE, 'bash', '-euo', 'pipefail', '-c',
        '''
test "$(id -u):$(id -g)" = 1001:1001
test "$(stat -c '%d:%i' "$PWD")" = "$EXPECTED_WORKSPACE_ID"
test "$(git rev-parse HEAD)" = "$EXPECTED_SHA"
test "$(rustc --version | cut -d ' ' -f 2)" = "$RUST_TOOLCHAIN"
test "$(node --version | cut -d. -f1)" = v22
npm --version
rustup target list --toolchain "$RUST_TOOLCHAIN" --installed | grep -Fx wasm32-unknown-unknown
rustup target list --toolchain "$OASIS7_WASM_TOOLCHAIN" --installed | grep -Fx "$OASIS7_WASM_TARGET"
rustup component list --toolchain "$OASIS7_WASM_TOOLCHAIN" --installed | grep -Fx rust-src
pkg-config --exists wayland-client xkbcommon x11 xi xcursor xrandr xinerama alsa libudev
if [[ "$1" == before-cross-uid ]]; then
  python3 -m venv "$HOME/doc-venv"
  "$HOME/doc-venv/bin/python" -m pip install --disable-pip-version-check --requirement scripts/doc-governance-requirements.txt
fi
export PATH="$HOME/doc-venv/bin:$CARGO_HOME/bin:$PATH"
exec ./scripts/ci-tests.sh full-support --full-support-phase "$1"
''', 'runtime-support', phase,
    ]
    try:
        subprocess.run(command, env=docker_env, check=True)
    finally:
        try:
            subprocess.run(['docker', 'rm', '--force', name], env=docker_env, timeout=10,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        except (OSError, subprocess.TimeoutExpired):
            print('Could not confirm cleanup of the owned runtime-support container', flush=True)


def run_host_fixture(source, owned, env):
    exact_source(ROOT, source)
    fixture = owned / 'host-fixture.py'
    fixture.write_bytes(git(ROOT, 'show', f'{source}:{FIXTURE}'))
    binary = owned / 'oasis7_game_client'
    digest = copy_native_elf(ROOT / BINARY, binary, os.getuid())
    print(f'Runtime support image ELF source={source} sha256={digest}', flush=True)
    result = subprocess.run(['sudo', '-n', 'timeout', '--signal=TERM', '--kill-after=5s', '60s',
                             '/usr/bin/python3', str(fixture), str(binary)],
                            env={'PATH': env['PATH']}, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, check=False)
    print(result.stdout, end='', flush=True)
    result.check_returncode()
    if f'Linux CLI sha256={digest}' not in result.stdout.splitlines():
        raise ValueError('host fixture did not observe the staged ELF hash')


def run_gate(source, env):
    if (not protected_context(env, source) or not supports_image(ROOT, source)
            or Path.cwd() != ROOT or (os.getuid(), os.getgid()) != (1001, 1001)
            or env.get('GITHUB_WORKSPACE') != str(ROOT) or env.get('RUNNER_TEMP') != str(TEMP)
            or any(env.get(key) for key in ('DOCKER_HOST', 'DOCKER_CONTEXT', 'DOCKER_TLS_VERIFY', 'DOCKER_CERT_PATH'))):
        raise ValueError('runtime support requires the original hosted release context and local Docker')
    parent = exact_source(ROOT, source)
    if (env.get('OASIS7_PRODUCT_DOC_BASE'), env.get('OASIS7_PRODUCT_DOC_HEAD')) != (parent, source):
        raise ValueError('runtime support requires the exact first-parent document range')
    canonical(TEMP)
    with tempfile.TemporaryDirectory(dir=TEMP, prefix='release-runtime-support-') as owned:
        scratch = Path(owned) / 'image-home'
        scratch.mkdir(mode=0o700)
        docker_config = Path(owned) / 'docker'
        docker_config.mkdir(mode=0o700)
        docker_env = {'PATH': env['PATH'], 'DOCKER_CONFIG': str(docker_config)}
        token = env.pop('GHCR_TOKEN')
        subprocess.run(['docker', 'login', 'ghcr.io', '--username', env['GITHUB_ACTOR'], '--password-stdin'],
                       env=docker_env, input=token, text=True, check=True)
        subprocess.run(['docker', 'pull', IMAGE], env=docker_env, check=True)
        image_phase('before-cross-uid', source, parent, scratch, docker_env)
        run_host_fixture(source, Path(owned), env)
        image_phase('after-cross-uid', source, parent, scratch, docker_env)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('select', 'run'))
    parser.add_argument('source')
    args = parser.parse_args()
    if args.mode == 'select':
        eligible = protected_context(os.environ, args.source) and supports_image(Path.cwd(), args.source)
        with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf-8') as output:
            print(f'enabled={str(eligible).lower()}', file=output)
    else:
        run_gate(args.source, os.environ)


if __name__ == '__main__':
    main()
