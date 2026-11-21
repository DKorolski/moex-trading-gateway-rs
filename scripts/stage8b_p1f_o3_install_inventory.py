#!/usr/bin/env python3
"""Read-only stopped O2 inventory for O3 installation. Never read key bytes."""
import json
import os
from pathlib import Path
import stat

import stage8b_p1f_o2_replay_install as prior

c = prior.c
OLD = '4ab7c2592c5427a0ca7aa1fc455caae0649a9e7652d36ee1d0d5feeeda055fef'
HEAD = 'fb4fa038d829f0d21e5f2d8e39b1bd26e05ba4f4186b95dfa6205b41d47933d8'
ADDED = tuple('/usr/local/libexec/moex/'+n for n in (
    'stage8b-p1f-local-supervisor', 'stage8b-p1f-o3-composition',
    'stage8b-p1f-o3-signer', 'stage8b-p1f-o3-provisioner')) + (
    c.BASE+'/moex-finam-p1f-o3-supervisor.service.in',
    c.BASE+'/moex-finam-p1f-o3-signer.service.in', c.BASE+'/installation-o3-v1.json')
ABSENT = (*prior.ABSENT, '/etc/moex-finam-p1-paper/o3',
    '/var/lib/moex-finam-p1-paper-o3-producer', '/run/moex-finam-p1-o3',
    '/run/moex-finam-p1-o3-input', '/run/moex-finam-p1-o3-runtime-credentials',
    '/run/credentials/moex-finam-p1f-o3-signer.service',
    '/etc/systemd/system/moex-finam-p1f-o3-signer.service',
    '/etc/systemd/system/moex-finam-p1f-o3-supervisor.service')
TREES = (*prior.TREES, prior.TX, '/root/moex-o2-replay-once-20261008')


def inventory(directory):
    """Pin public durable bytes including service-owned state; secret metadata only.

    Stopped host is checked before/after; stable inode/size/timestamps during
    read are checked. No symlink, hardlink or special file is followed.
    """
    c.require(directory.is_dir() and not directory.is_symlink(), 'inventory directory')
    out = {}
    for p in (directory, *sorted(directory.rglob('*'))):
        s = p.lstat()
        c.require(len(out) < 8000 and (stat.S_ISDIR(s.st_mode) or stat.S_ISREG(s.st_mode)), 'unsafe inventory node')
        value = dict(uid=s.st_uid, gid=s.st_gid, mode=oct(stat.S_IMODE(s.st_mode)), type=stat.S_IFMT(s.st_mode))
        if stat.S_ISREG(s.st_mode):
            c.require(s.st_nlink == 1 and s.st_size <= 64*1024*1024, 'inventory hardlink/size')
            value.update(size=s.st_size, nlink=s.st_nlink)
            secret = ('credentials' in p.parts or p.name == 'retired-2'
                      or p.suffix in ('.key', '.ed25519') or 'auth.private' in p.name)
            if secret:
                value.update(content_not_read=True, inode=s.st_ino, device=s.st_dev,
                             mtime_ns=s.st_mtime_ns, ctime_ns=s.st_ctime_ns)
            else:
                fd = os.open(p, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
                with os.fdopen(fd, 'rb') as f:
                    c.require(os.fstat(f.fileno()) == s, 'inventory replaced')
                    raw = f.read(s.st_size+1)
                    after = os.fstat(f.fileno())
                c.require(p.lstat() == after and len(raw) == s.st_size and
                          (s.st_mtime_ns, s.st_ctime_ns) == (after.st_mtime_ns, after.st_ctime_ns), 'inventory read changed')
                value['sha256'] = c.sha(raw)
        out[str(p.relative_to(directory))] = value
    return out


def observe(root):
    path = lambda n: c.old.rooted(root, n)
    host = prior.host_observation()
    head = prior.parse(c.file_bytes(root, c.CONTROL+'/authority/history-head.json', 0o440, 987))
    c.require(head['state'] == 'COMPLETED' and type(head['latest_sequence']) is int
              and head['latest_sequence'] == 26 and type(head['authority_generation']) is int
              and head['authority_generation'] == 1 and head['latest_event_sha256'] == HEAD
              and head['active_manifest_sha256'] is None and head['deadline_utc'] is None,
              'exact accepted O2 COMPLETED/1/26 required')
    c.require(all(not os.path.lexists(path(n)) for n in ABSENT), 'operational input/unit already present')
    trees = {n: inventory(path(n)) for n in TREES}
    c.require(host == prior.host_observation(), 'host changed while collecting')
    return dict(history_head=head, trees=trees, host=host)


def collect():
    root = Path('/')
    before = observe(root)
    raw = c.file_bytes(root, c.MANIFEST, 0o644, 0)
    c.require(c.sha(raw) == OLD, 'installed O2 predecessor drift')
    old = prior.parse(raw)
    installed = {}; public = {c.MANIFEST: raw.decode()}
    for n, e in old['payload'].items():
        raw = c.file_bytes(root, n, int(e['mode'], 8), 987 if e['group']=='service' else 0)
        c.require(c.sha(raw) == e['sha256'] and len(raw) == e['size'], 'installed bytes drift: '+n)
        installed[n] = e
        if n not in prior.ELFS:
            c.require('/credentials/' not in n and not n.endswith(('.key','.ed25519')), 'secret payload')
            public[n] = raw.decode()
    c.require(all(not os.path.lexists(n) for n in ADDED), 'new slot exists')
    c.require(before == observe(root), 'state changed while collecting')
    return dict(status='PASS_READ_ONLY', expected=before, installed=installed,
        old_public_payload=public, new_slots_absent=list(ADDED),
        mutation_performed=False, private_bytes_read=False, redis_contact=False, finam_contact=False)


if __name__ == '__main__':
    print(json.dumps(collect(), sort_keys=True))
