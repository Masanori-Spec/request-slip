"""CI only: pinned official tools, registry provenance and license metadata.
No downloaded source or binary is part of the source distribution/artifact.
"""
from pathlib import Path
import hashlib, io, json, os, re, shutil, subprocess, tarfile, tomllib, urllib.request

ROOT = Path(__file__).resolve().parent.parent
PIN = json.loads((ROOT / 'scripts/upstream-pin.json').read_text())
OUT = ROOT / 'evidence'
OUT.mkdir(exist_ok=True)
LOCKED = ['--locked'] if (ROOT / 'Cargo.lock').exists() else []
metadata = json.loads(subprocess.check_output(['cargo', '+1.98.1', 'metadata', *LOCKED, '--format-version', '1'], cwd=ROOT))
core, = [p for p in metadata['packages'] if p['name'] == 'hurl_core']
assert core['version'] == PIN['version'] and core['license'] == 'Apache-2.0'
source = Path(core['manifest_path']).parent
vcs = json.loads((source / '.cargo_vcs_info.json').read_text())
assert vcs['git']['sha1'] == PIN['commit'] and not vcs['git'].get('dirty', False)
files = []
for item in PIN['source_files']:
    local = source / Path(item['path']).relative_to('packages/hurl_core')
    data = local.read_bytes()
    assert len(data) == item['bytes']
    assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == item['git_blob_sha']
    files.append({**item, 'sha256': hashlib.sha256(data).hexdigest()})
lock = ROOT / 'Cargo.lock'
tuples = {(p['name'], p['version']): p.get('checksum') for p in tomllib.loads(lock.read_text())['package']}
allowed = {'MIT', 'Apache-2.0', 'MPL-2.0', 'BSD-2-Clause', 'BSD-3-Clause', 'ISC', 'Zlib', 'Unicode-3.0', 'Unlicense', '0BSD', 'MIT-0'}
deps = []
for p in metadata['packages']:
    if p['name'] == 'request-slip':
        continue
    assert p['source'].startswith('registry+'), 'Only registry dependencies are allowed'
    license_text = p.get('license') or ''
    atoms = set(re.findall(r'[A-Za-z0-9][A-Za-z0-9.-]*', license_text)) - {'OR', 'AND'}
    assert atoms and atoms <= allowed, f'Unreviewed license: {p["name"]}: {license_text}'
    deps.append({'name': p['name'], 'version': p['version'], 'license': license_text, 'checksum': tuples[(p['name'], p['version'])]})
shutil.copyfile(lock, OUT / 'resolved-Cargo.lock')
provenance = {'core': PIN['crate'], 'version': PIN['version'], 'commit': PIN['commit'], 'files': files, 'dependencies': deps, 'lock_sha256': hashlib.sha256(lock.read_bytes()).hexdigest(), 'libxml2_version': subprocess.check_output(['pkg-config', '--modversion', 'libxml-2.0'], text=True).strip(), 'distribution': 'Original CLI/test source plus metadata only; no dependency source or binary bundled'}
(OUT / 'native-provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
asset = PIN['official_linux_asset']
with urllib.request.urlopen(asset['url'], timeout=60) as response:
    data = response.read(asset['bytes'] + 1)
assert len(data) == asset['bytes'] and hashlib.sha256(data).hexdigest() == asset['sha256']
dest = ROOT / '.native/bin'
dest.mkdir(parents=True, exist_ok=True)
with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as tar:
    for name in ['hurl', 'hurlfmt']:
        matches = [m for m in tar.getmembers() if Path(m.name).name == name and m.isfile()]
        member, = matches
        assert member.size < 32 * 1024 * 1024
        payload = tar.extractfile(member).read()
        target = dest / name
        target.write_bytes(payload)
        target.chmod(0o755)
versions = {n: subprocess.check_output([str(dest / n), '--version'], text=True).splitlines()[0] for n in ['hurl', 'hurlfmt']}
assert all('8.0.1' in s for s in versions.values())
(OUT / 'official-tool-identity.json').write_text(json.dumps({'asset': asset, 'versions': versions, 'binary_sha256': {n: hashlib.sha256((dest / n).read_bytes()).hexdigest() for n in versions}}, indent=2) + '\n')
print(f'Verified {len(files)} official source files and {len(deps)} dependency license/checksum records; official Hurl/hurlfmt8.0.1 asset hash passed')
