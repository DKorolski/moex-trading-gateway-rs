"""One-shot public package staging only; not an installer or activation command."""
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import stat
import zipfile

base = Path('/root/stage8b-p1f-o2-install-304cd56')
name = 'moex-trading-project-304cd56-stage8b-p1f-o2-installation-package.zip'
expected = '22c034decddbd3062a40b1d6a95b71d5beec222c2298f83f112659890ef68bbc'
assert os.geteuid() == 0
for parent in (Path('/'), Path('/root'), base):
    s = parent.lstat()
    assert stat.S_ISDIR(s.st_mode) and s.st_uid == s.st_gid == 0 and not stat.S_IMODE(s.st_mode) & 0o022
assert stat.S_IMODE(base.stat().st_mode) == 0o700
path = base / name
s = path.lstat()
assert stat.S_ISREG(s.st_mode) and s.st_uid == s.st_gid == 0 and s.st_nlink == 1
raw = path.read_bytes()
assert hashlib.sha256(raw).hexdigest() == expected
path.chmod(0o600)
destination = base / 'bundle'
assert not destination.exists() and not destination.is_symlink()
with zipfile.ZipFile(io.BytesIO(raw)) as archive:
    entries = archive.infolist()
    assert len(entries) == len({i.filename for i in entries}) == 2498
    for entry in entries:
        parts = entry.filename.split('/')
        assert not PurePosixPath(entry.filename).is_absolute()
        assert not any(p in ('', '.', '..') for p in parts) and '\\' not in entry.filename
        assert entry.external_attr >> 16 in (0o100644, 0o100755)
    assert archive.testzip() is None
    os.umask(0o077)
    destination.mkdir(mode=0o700)
    for entry in entries:
        target = destination / entry.filename
        target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        data = archive.read(entry)
        with target.open('xb') as output:
            output.write(data)
        target.chmod((entry.external_attr >> 16) & 0o777)
        s = target.lstat()
        assert s.st_uid == s.st_gid == 0 and s.st_nlink == 1
        assert stat.S_IMODE(s.st_mode) == (entry.external_attr >> 16) & 0o777
        assert target.read_bytes() == data
print(json.dumps({'result': 'EXACT_PACKAGE_STAGED', 'archive_sha256': expected,
                  'members_verified': len(entries), 'root_only_staging': str(base),
                  'installed_payload_changed': False, 'activation_performed': False}, sort_keys=True))
