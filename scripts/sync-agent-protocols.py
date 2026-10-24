#!/usr/bin/env python3
"""Adopt committed Agent Protocols fixtures, or verify the offline baseline."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import re

ROOT = Path(__file__).resolve().parent.parent
DEST = ROOT / 'tests/vectors/agent-protocols'
FILES = {
    f'agent-{name}-1.0.{kind}.json': f'docs/protocols/agent-{name}/1.0.{kind}.json'
    for name, kinds in {
        'identity': ['vectors'], 'profile': ['vectors'], 'delegation': ['vectors'],
        'discourse': ['vectors'], 'knowledge': ['vectors', 'schema'], 'mail': ['vectors', 'schema']
    }.items()
    for kind in kinds
}
FILES['LICENSE'] = 'LICENSE'

def version():
    # These repository-owned manifests use one inline workspace dependency and
    # standard Cargo.lock package sections; no optional TOML package is needed.
    cargo = (ROOT / 'Cargo.toml').read_text().split('[workspace.dependencies]', 1)[1].split('\n[', 1)[0]
    declaration = re.search(r'(?m)^agent-protocols\s*=\s*\{([^}]+)\}', cargo)
    if declaration is None:
        raise ValueError('missing workspace agent-protocols dependency')
    expected = re.search(r'version\s*=\s*"([^"]+)"', declaration[1])[1].removeprefix('=')
    installed = []
    for package in (ROOT / 'Cargo.lock').read_text().split('[[package]]')[1:]:
        if re.search(r'(?m)^name = "agent-protocols"$', package):
            installed.append(re.search(r'(?m)^version = "([^"]+)"$', package)[1])
    if installed != [expected]:
        raise ValueError('Cargo declaration and lockfile must agree on one agent-protocols version')
    for package in ['packages/dmsg-sdk', 'src/dmsg_desktop']:
        declared = json.loads((ROOT / package / 'package.json').read_text())['dependencies']['agent-protocols']
        if declared != expected:
            raise ValueError(f'{package} must pin agent-protocols {expected}')
    return expected

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--check', action='store_true')
    mode.add_argument('--update', action='store_true')
    parser.add_argument('--source', type=Path, help='path to the committed upstream checkout')
    args = parser.parse_args()
    expected = version()
    source = args.source.resolve() if args.source else None
    if args.update and source is None:
        parser.error('--update requires --source')
    commit = None
    if source:
        upstream = json.loads((source / 'packages/agent-protocols/package.json').read_text())['version']
        if upstream != expected:
            raise ValueError(f'upstream is {upstream}; workspace expects {expected}')
        if subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=source).strip():
            raise ValueError('commit upstream changes before adopting fixtures')
        commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip()
    manifest_path = DEST / 'source.json'
    if args.update:
        manifest = {'version': expected, 'commit': commit, 'files': {}}
        for name, upstream_path in FILES.items():
            data = (source / upstream_path).read_bytes()
            (DEST / name).write_bytes(data)
            manifest['files'][name] = hashlib.sha256(data).hexdigest()
        manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
    else:
        manifest = json.loads(manifest_path.read_text())
        if manifest['version'] != expected or set(manifest['files']) != set(FILES):
            raise ValueError('adopt the fixtures for the declared SDK version')
        for name, upstream_path in FILES.items():
            data = (DEST / name).read_bytes()
            if hashlib.sha256(data).hexdigest() != manifest['files'][name]:
                raise ValueError(f'fixture checksum mismatch: {name}')
            if source and data != (source / upstream_path).read_bytes():
                raise ValueError(f'fixture differs from upstream: {name}')
    print(f'Agent Protocols {expected}: {len(FILES) - 1} normative files and MIT license verified')

if __name__ == '__main__':
    main()
