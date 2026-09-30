#!/usr/bin/env python3
"""One fixed non-activating O2 update. Preserve exact accepted terminal history.

No empty-root installer, signing, activation, systemd reload/start or network API.
Operational invocation requires independent package acceptance and permission.
"""
import argparse
import contextlib
import fcntl
import io
import json
import os
from pathlib import Path
import stat
import sys
import zipfile

import stage8b_p1f_o2_install as custody

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / 'docs/stage-8/stage8b-p1f-o2-terminal-update.json'
TRANSACTION = custody.BASE + '/o2-terminal-successor-589b801'
OLD_SHA = '316c2376cf4d02a7f0ee3837e96d93bbf2cb1b2b8e3aabe10205f783f088aad8'
TERMINAL_SHA = '828fb41cabc590054590a1744587f71cc84b603fff5d44d3021fcdef841e813b'
CONFIRM = 'UPDATE_O2_AFTER_EXPIRED_2_WITHOUT_ACTIVATION'
ROLLBACK = 'ROLLBACK_O2_UPDATE_PRESERVING_EXPIRED_2'
CHANGES = (
    '/usr/local/libexec/moex/stage8b-p1f-o2-materializer',
    '/usr/local/libexec/moex/stage8b-p1f-o2-operator',
    '/usr/local/libexec/moex/stage8b-p1-paper-supervisor',
    '/etc/systemd/system/moex-finam-p1f-o2-materializer.service',
    custody.old.MANIFEST, custody.MANIFEST,
)
require = custody.require

def strict_json(raw):
    def pairs(rows):
        obj = {}
        for key, value in rows:
            require(key not in obj, 'duplicate JSON key')
            obj[key] = value
        return obj
    return json.loads(raw, object_pairs_hook=pairs)

def inventory(root):
    """Hashes/custody only; never export private document contents."""
    require(root.is_dir() and not root.is_symlink(), 'missing/symlink invariant directory')
    result = {}
    for path in [root, *sorted(root.rglob('*'))]:
        require(len(result) < 2000, 'inventory limit')
        s = path.lstat()
        require(stat.S_ISREG(s.st_mode) or stat.S_ISDIR(s.st_mode), 'unsafe invariant node')
        value = dict(uid=s.st_uid, gid=s.st_gid, mode=oct(stat.S_IMODE(s.st_mode)),
                     type=stat.S_IFMT(s.st_mode), nlink=s.st_nlink, size=s.st_size)
        if stat.S_ISREG(s.st_mode):
            require(s.st_nlink == 1 and s.st_size <= 64 * 1024 * 1024, 'unsafe invariant file')
            fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
            try:
                opened = os.fstat(fd)
                require((s.st_dev, s.st_ino) == (opened.st_dev, opened.st_ino), 'invariant replaced')
                with os.fdopen(os.dup(fd), 'rb') as stream:
                    raw = stream.read(s.st_size + 1)
                after = os.fstat(fd)
                require(len(raw) == s.st_size and (s.st_mtime_ns, s.st_ctime_ns) ==
                        (after.st_mtime_ns, after.st_ctime_ns), 'invariant changed during read')
                value['sha256'] = custody.sha(raw)
            finally:
                os.close(fd)
        result[str(path.relative_to(root))] = value
    return result

class Update:
    def __init__(self, root, payload, spec, expected):
        self.root = custody.old.validate_root(root)
        self.payload = payload
        self.spec = spec
        self.expected = expected
        self.uid, self.gid = custody.old.account_ids(root)
        require(spec['execution_authorized'] is False, 'execution scope opened')
        require(spec['old_manifest_sha256'] == OLD_SHA, 'predecessor changed')
        require(set(payload) == set(CHANGES), 'write inventory drift')
        require(set(spec['updates']) == set(CHANGES), 'update identity drift')
        for path in CHANGES:
            require(custody.sha(payload[path]) == spec['updates'][path]['new_sha256'], 'payload drift')
        require(custody.sha(payload[custody.MANIFEST]) == spec['new_manifest_sha256'], 'new installation binding')
        self.new = strict_json(payload[custody.MANIFEST])
        self.old = spec['old_inventory']
        require(custody.sha(custody.canonical(self.old)) == OLD_SHA, 'old inventory drift')
        mutable = {'artifact_ref', 'artifact_sha256', 'installer_sha256', 'predecessor_manifest_sha256', 'payload'}
        require(set(self.new) == set(self.old) | {'update_revision'}, 'installation schema drift')
        require(all(self.new[k] == self.old[k] for k in self.old if k not in mutable), 'installation boundary drift')
        require(self.new['artifact_ref'] == spec['compiled_source_ref'] == '589b80144adaa4c615aaa94781035d5a6af64c71'
                and self.new['artifact_sha256'] == spec['artifact_sha256']
                and self.new['installer_sha256'] == custody.sha(Path(__file__).read_bytes())
                and self.new['predecessor_manifest_sha256'] == OLD_SHA, 'update/source lineage drift')
        require(self.new['installation_id'] == self.old['installation_id'] == custody.INSTALLATION_ID, 'genesis identity must be retained')
        require(set(self.new['payload']) == set(self.old['payload']), 'managed inventory changed')
        require(self.new['directories'] == self.old['directories'], 'directory identity changed')
        for name, old in self.old['payload'].items():
            entry = self.new['payload'][name]
            require(set(entry) == set(old), 'payload metadata schema drift')
            require(all(entry[k] == old[k] for k in ('mode', 'owner', 'group')), 'custody must not change')
            if name in CHANGES:
                require(entry['sha256'] == custody.sha(payload[name]) and entry['size'] == len(payload[name]), 'new inventory binding')
            else:
                require(entry == old, 'unlisted payload change')
        for name, update in spec['updates'].items():
            require(update['old_sha256'] == (OLD_SHA if name == custody.MANIFEST else self.old['payload'][name]['sha256']), 'old slot binding')

    def path(self, name):
        return custody.old.rooted(self.root, name)

    def read(self, name):
        entry = self.old['payload'].get(name, {'mode': '0644', 'group': 'root'})
        return custody.file_bytes(self.root, name, int(entry['mode'], 8), self.gid if entry['group'] == 'service' else 0)

    def invariants(self):
        observed = custody.observe(self.root)
        require(observed == self.expected['observation'], 'host/P0/stopped observation drift')
        for name, key in ((custody.CONTROL, 'authority_inventory'),
                          ('/etc/moex-finam-p1-paper', 'config_inventory'),
                          ('/var/lib/moex-finam-p1-paper/state', 'durable_inventory'),
                          (custody.STAGING, 'staging_inventory')):
            require(inventory(self.path(name)) == self.expected[key], 'terminal/selector/materialization invariant drift: ' + key)
        for name in ('/run/moex-finam-p1f-o2-input', '/run/credentials/moex-finam-p1f-o2-materializer.service'):
            require(not custody.exists(self.root, name), 'transient credential exists')
        return observed

    def verify_files(self, state):
        for name, old in self.old['payload'].items():
            digest = custody.sha(self.read(name))
            allowed = {old['sha256']}
            if name in CHANGES:
                new = self.spec['updates'][name]['new_sha256']
                allowed = {'old': {old['sha256']}, 'new': {new}, 'mixed': {old['sha256'], new}}[state]
            require(digest in allowed, 'foreign installed bytes: ' + name)
        digest = custody.sha(self.read(custody.MANIFEST))
        allowed = {'old': {OLD_SHA}, 'new': {self.spec['new_manifest_sha256']},
                   'mixed': {OLD_SHA, self.spec['new_manifest_sha256']}}[state]
        require(digest in allowed, 'foreign installation manifest')

    @contextlib.contextmanager
    def locks(self):
        # Existing inodes only. No authority file, selector or history write.
        fds = []
        try:
            for name in ('.execution.lock', '.guardian.lock'):
                path = self.path(custody.CONTROL + '/' + name)
                custody.old.verify_secure_directory_chain(self.root, path.parent)
                fd = os.open(path, os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK)
                fds.append(fd)
                s = os.fstat(fd)
                require(stat.S_ISREG(s.st_mode) and s.st_uid == s.st_gid == 0 and s.st_nlink == 1
                        and stat.S_IMODE(s.st_mode) == 0o600, 'lock custody drift')
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            yield
        finally:
            for fd in reversed(fds):
                os.close(fd)

    def journal(self):
        directory = self.path(TRANSACTION).lstat()
        require(stat.S_ISDIR(directory.st_mode) and directory.st_uid == directory.st_gid == 0
                and stat.S_IMODE(directory.st_mode) == 0o700, 'transaction directory custody')
        require({p.name for p in self.path(TRANSACTION).iterdir()} ==
                {'journal.json', *[f'before-{i}' for i in range(len(CHANGES))]}, 'transaction inventory drift')
        value = strict_json(custody.file_bytes(self.root, TRANSACTION + '/journal.json', 0o600, 0))
        require(set(value) == {'domain', 'old_sha256', 'new_sha256', 'state'}, 'journal schema')
        require(value['domain'] == 'o2-terminal-successor-589b801-v1'
                and value['old_sha256'] == OLD_SHA and value['new_sha256'] == self.spec['new_manifest_sha256']
                and value['state'] in {'PREPARED', 'APPLIED', 'ROLLING_BACK', 'ROLLED_BACK'}, 'journal identity/state')
        for i, name in enumerate(CHANGES):
            raw = custody.file_bytes(self.root, TRANSACTION + f'/before-{i}', 0o600, 0)
            require(custody.sha(raw) == self.spec['updates'][name]['old_sha256'], 'backup corruption')
        return value

    def write_journal(self, state):
        value = dict(domain='o2-terminal-successor-589b801-v1', old_sha256=OLD_SHA,
                     new_sha256=self.spec['new_manifest_sha256'], state=state)
        custody.atomic_write(self.root, TRANSACTION + '/journal.json', custody.canonical(value), 0o600, 0)

    def transition(self, action):
        require(action in {'preflight', 'apply', 'resume', 'rollback'}, 'unknown action')
        with self.locks():
            self.invariants()
            transaction = self.path(TRANSACTION)
            if not custody.exists(self.root, TRANSACTION):
                self.verify_files('old')
                if action == 'preflight':
                    return {'state': 'EXACT_OLD_TERMINAL', 'history_preserved': True}
                require(action == 'apply', 'no prior update transaction')
                custody.old.verify_secure_directory_chain(self.root, transaction.parent)
                transaction.mkdir(mode=0o700)
                custody.sync_dir(transaction.parent)
                for i, name in enumerate(CHANGES):
                    custody.atomic_write(self.root, TRANSACTION + f'/before-{i}', self.read(name), 0o600, 0)
                self.write_journal('PREPARED')
            j = self.journal()
            current = {'APPLIED': 'new', 'ROLLED_BACK': 'old'}.get(j['state'], 'mixed')
            self.verify_files(current)
            if action == 'preflight':
                return {'state': j['state'], 'history_preserved': True}
            if action in {'apply', 'resume'}:
                require(j['state'] in {'PREPARED', 'APPLIED'}, 'cannot resume rolled-back transaction')
                desired, terminal = 'new', 'APPLIED'
            else:
                desired, terminal = 'old', 'ROLLED_BACK'
                if j['state'] != 'ROLLED_BACK':
                    self.write_journal('ROLLING_BACK')
            for i, name in enumerate(CHANGES):
                self.invariants()
                self.verify_files('mixed')
                raw = self.payload[name] if desired == 'new' else custody.file_bytes(self.root, TRANSACTION + f'/before-{i}', 0o600, 0)
                if self.read(name) != raw:
                    mode = int(self.old['payload'].get(name, {'mode': '0644'})['mode'], 8)
                    custody.atomic_write(self.root, name, raw, mode, 0)
            self.verify_files(desired)
            self.invariants()
            self.write_journal(terminal)
            return {'state': terminal, 'history_preserved': True, 'activation_performed': False,
                    'daemon_reload_performed': False, 'redis_contact': False, 'finam_contact': False}

def load_inputs():
    spec = strict_json(SPEC.read_bytes())
    evidence = ROOT / 'accepted-evidence/finam-o2-43d5f4e-terminal-recovery-evidence-20260930.zip'
    raw = evidence.read_bytes()
    require(custody.sha(raw) == TERMINAL_SHA, 'accepted terminal evidence mismatch')
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        expected = strict_json(archive.read('evidence/postflight.stdout'))
    require(expected['status'] == 'PASS' and expected['authority_inspection']['state'] == 'EXPIRED'
            and expected['authority_inspection']['latest_sequence'] == 2, 'terminal frontier mismatch')
    payload = {name: (ROOT / 'update-payload' / Path(name).name).read_bytes() for name in CHANGES}
    return spec, expected, payload

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('preflight', 'apply', 'resume', 'rollback'))
    parser.add_argument('--confirm')
    args = parser.parse_args()
    try:
        require(os.geteuid() == 0, 'root required')
        require(args.confirm == (None if args.action == 'preflight' else ROLLBACK if args.action == 'rollback' else CONFIRM), 'exact operational confirmation required')
        spec, expected, payload = load_inputs()
        print(json.dumps(Update(Path('/'), payload, spec, expected).transition(args.action), sort_keys=True))
    except (custody.Error, OSError, ValueError, KeyError, zipfile.BadZipFile) as error:
        print('o2-terminal-update: FAIL ' + str(error), file=sys.stderr)
        raise SystemExit(1)
