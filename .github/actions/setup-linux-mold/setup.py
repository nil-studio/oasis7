#!/usr/bin/env python3
"""Install the pinned Linux linker; this action also runs without a checkout."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import tarfile
import tempfile
import time


VERSION = '3.0.0'
# The directory is part of Cargo/cache identity. Bump v1 if the driver changes.
INSTALL_DIR = Path('/opt/oasis7-mold') / (VERSION + '-v1')
RELEASES = {
    'x86_64': ('x86_64-unknown-linux-gnu',
               '6c90d4a474c7c0409dfb575be03a5345878ac14fdba18de8b40fa58c60121189'),
    'aarch64': ('aarch64-unknown-linux-gnu',
                '52c759d3689babaea4c42af2f31062a74ab83e17ffb5a8090232534c67b70577'),
}


def run(command, **kwargs):
    return subprocess.run(command, check=True, text=True, timeout=30, **kwargs)


def platform_release():
    arch = platform.machine()
    if platform.system() != 'Linux' or arch not in RELEASES:
        raise ValueError('mold setup supports Linux x86_64 and aarch64 hosts')
    triple, checksum = RELEASES[arch]
    return arch, triple, checksum


def verify_archive(archive, checksum):
    digest = hashlib.sha256()
    with archive.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    if digest.hexdigest() != checksum:
        raise ValueError('mold release archive SHA-256 mismatch')


def extract_file(archive, name, destination):
    # Extract only known regular files; never apply archive paths or symlinks.
    member = archive.getmember(name)
    if not member.isfile():
        raise ValueError('mold release member is not a regular file: ' + name)
    with archive.extractfile(member) as source, destination.open('wb') as output:
        shutil.copyfileobj(source, output)


def prepare_directory(directory):
    try:
        directory.mkdir(parents=True, exist_ok=True)
    except PermissionError:
        if directory != INSTALL_DIR:
            raise
        # Give the current runner ownership of this version only, not /opt or ld.
        run(['sudo', '-n', 'install', '-d', '-m', '755', '-o', str(os.getuid()),
             '-g', str(os.getgid()), str(directory)])
    (directory / 'bin').mkdir(exist_ok=True)
    (directory / 'share/doc/mold').mkdir(parents=True, exist_ok=True)


def install_file(source, destination, mode):
    with tempfile.NamedTemporaryFile(prefix=destination.name + '.', dir=destination.parent,
                                     delete=False) as output:
        temporary = Path(output.name)
    try:
        shutil.copyfile(source, temporary)
        temporary.chmod(mode)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)


def smoke(linker, directory):
    binary = directory / 'native-smoke'
    started = time.monotonic()
    # An earlier lld selection must not defeat the driver's final mold selection.
    run([str(linker), '-fuse-ld=lld', '-x', 'c', '-', '-o', str(binary)],
        input='int main(void) { return 0; }\n')
    comment = run(['readelf', '-p', '.comment', str(binary)], capture_output=True).stdout
    if not re.search(r'\bmold ' + re.escape(VERSION) + r'(?:\s|$)', comment):
        raise ValueError('native smoke ELF was not linked by the pinned mold')
    run([str(binary)])
    return round(time.monotonic() - started, 3)


def append_actions_file(variable, text):
    if os.environ.get(variable):
        with Path(os.environ[variable]).open('a', encoding='utf-8') as output:
            output.write(text)


def resolve_cache_directory(cache_dir=None):
    if cache_dir is not None:
        return Path(cache_dir).expanduser()
    env_cache = os.environ.get('OASIS7_MOLD_CACHE_DIR')
    if env_cache:
        return Path(env_cache).expanduser()
    return None


def download_release_archive(destination, arch, checksum):
    url = (f'https://github.com/rui314/mold/releases/download/v{VERSION}/'
           f'mold-{VERSION}-{arch}-linux.tar.gz')
    subprocess.run(['curl', '--fail', '--location', '--silent', '--show-error',
                    '--retry', '3', '--retry-all-errors', '--retry-connrefused',
                    '--retry-max-time', '120', '--connect-timeout', '15',
                    '--max-time', '120', '--output', str(destination), url],
                   check=True, timeout=150)
    verify_archive(destination, checksum)


def install(directory, archive=None, cache_dir=None):
    started = time.monotonic()
    arch, triple, checksum = platform_release()
    directory = directory.absolute()
    if any(character in str(directory) for character in '\r\n'):
        raise ValueError('mold install directory must fit on one environment line')
    gcc = shutil.which('gcc')
    if not gcc or not shutil.which('readelf'):
        raise ValueError('mold setup requires GCC and readelf before Rust installation')
    bin_directory = directory / 'bin'
    linker = bin_directory / 'oasis7-mold-gcc'
    key = 'CARGO_TARGET_' + triple.upper().replace('-', '_') + '_LINKER'
    if os.environ.get(key) not in (None, '', str(linker)):
        raise ValueError('mold setup conflicts with existing ' + key)

    resolved_cache = resolve_cache_directory(cache_dir)
    cache_archive = resolved_cache / f'mold-{VERSION}-{arch}-linux.tar.gz' if resolved_cache else None
    cache_hit = False

    archive_to_extract = archive
    if archive_to_extract is not None:
        verify_archive(archive_to_extract, checksum)
    elif cache_archive and cache_archive.is_file():
        try:
            verify_archive(cache_archive, checksum)
            archive_to_extract = cache_archive
            cache_hit = True
        except ValueError:
            cache_archive.unlink(missing_ok=True)

    with tempfile.TemporaryDirectory(prefix='oasis7-mold-') as temporary:
        stage = Path(temporary)
        if archive_to_extract is None:
            downloaded = stage / 'release.tar.gz'
            download_release_archive(downloaded, arch, checksum)
            archive_to_extract = downloaded
            if cache_archive:
                try:
                    resolved_cache.mkdir(parents=True, exist_ok=True)
                    temp_cache = resolved_cache / f'.mold-{os.getpid()}.tar.gz'
                    shutil.copyfile(downloaded, temp_cache)
                    temp_cache.replace(cache_archive)
                except OSError:
                    pass

        with tarfile.open(archive_to_extract, 'r:gz') as release:
            prefix = f'mold-{VERSION}-{arch}-linux/'
            extract_file(release, prefix + 'bin/mold', stage / 'mold')
            extract_file(release, prefix + 'share/doc/mold/LICENSE', stage / 'LICENSE')
        (stage / 'mold').chmod(0o755)
        version = run([str(stage / 'mold'), '--version'], capture_output=True).stdout.strip()
        if not re.match(r'^mold ' + re.escape(VERSION) + r'(?:\s|$)', version):
            raise ValueError('mold binary version does not match the pinned release')
        prepare_directory(directory)
        install_file(stage / 'mold', bin_directory / 'mold', 0o755)
        install_file(stage / 'LICENSE', directory / 'share/doc/mold/LICENSE', 0o644)
        alias = bin_directory / f'.ld.mold.{os.getpid()}'
        alias.unlink(missing_ok=True)
        alias.symlink_to('mold')
        alias.replace(bin_directory / 'ld.mold')
        # The -gcc suffix makes rustc select a GNU compiler driver, not rust-lld.
        wrapper = ('#!/usr/bin/env bash\nset -euo pipefail\n'
                   f'export PATH={shlex.quote(str(bin_directory))}:"$PATH"\n'
                   f'exec {shlex.quote(gcc)} "$@" -fuse-ld=mold\n')
        (stage / 'driver').write_text(wrapper, encoding='utf-8')
        install_file(stage / 'driver', linker, 0o755)
        smoke_seconds = smoke(linker, stage)
    elapsed = round(time.monotonic() - started, 3)
    # Publish configuration only after checking an actual linked, runnable ELF.
    append_actions_file('GITHUB_ENV', f'{key}={linker}\n')
    append_actions_file('GITHUB_OUTPUT',
                        f'linker={linker}\nversion={VERSION}\nsetup-seconds={elapsed}\ncache-hit={str(cache_hit).lower()}\n')
    append_actions_file('GITHUB_STEP_SUMMARY',
                        f'\n### Linux native linker\n\n'
                        f'- mold `{VERSION}`, target `{triple}`\n'
                        f'- Setup: {elapsed}s (cache hit: {cache_hit}); C link and execution smoke: {smoke_seconds}s\n'
                        '- Native Cargo linker configured; WASM target linker unchanged.\n')
    return dict(version=VERSION, binary_version=version, target=triple, linker=str(linker),
                environment={key: str(linker)},
                archive_sha256=checksum, setup_seconds=elapsed, smoke_seconds=smoke_seconds,
                cache_hit=cache_hit)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install-dir', type=Path, default=INSTALL_DIR,
                        help='versioned installation directory (override for local verification)')
    parser.add_argument('--archive', type=Path,
                        help='previously downloaded official archive; SHA-256 is always verified')
    parser.add_argument('--cache-dir', type=Path,
                        help='cache directory for the verified release archive (default: OASIS7_MOLD_CACHE_DIR)')
    parser.add_argument('--json', action='store_true', help='print only the JSON setup report')
    args = parser.parse_args()
    try:
        report = install(args.install_dir, args.archive, cache_dir=args.cache_dir)
    except (OSError, ValueError, KeyError, tarfile.TarError, subprocess.SubprocessError) as error:
        parser.exit(1, 'mold setup failed: ' + str(error) + '\n')
    print(('' if args.json else 'linux_mold=') + json.dumps(report, sort_keys=True))


if __name__ == '__main__':
    main()
