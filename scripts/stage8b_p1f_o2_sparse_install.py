#!/usr/bin/env python3
"""Fixed no-riskgate update after FAILED/6; same bounded custody/transaction protocol.

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
SPEC = ROOT / 'installation-sparse/spec.json'
TRANSACTION = custody.BASE + '/o2-sparse-4668b42'
OLD_SHA = '5b1ed8cc5553694d836cc0fbf2c5b9855cc46f08da88f7c3ac46e85c7aac1ed2'
TERMINAL_SHA = '0fa9ad27adc850c929ae709b9fc6f543cb6d36aa4315523d2417a745d59bb8f8'
CONFIRM = 'INSTALL_SPARSE_AFTER_FAILED_6_WITHOUT_ACTIVATION'
ROLLBACK = 'ROLLBACK_SPARSE_PRESERVING_FAILED_6'
CHANGES = (
    '/usr/local/libexec/moex/stage8b-p1f-o2-materializer',
    '/usr/local/libexec/moex/stage8b-p1f-o2-operator',
    '/usr/local/libexec/moex/stage8b-p1-paper-supervisor',
    custody.O2DIR + '/materialization-policy.json',
    custody.O2DIR + '/source-template.json',
    custody.O2DIR + '/supervisor.template.json',
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
        require(expected['history_head']['state'] == 'FAILED'
                and type(expected['history_head']['latest_sequence']) is int
                and expected['history_head']['latest_sequence'] == 6
                and type(expected['history_head']['authority_generation']) is int
                and expected['history_head']['authority_generation'] == 1,
                'expected FAILED/1/6 frontier required')
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
        mutable = {'artifact_ref', 'artifact_sha256', 'installer_sha256', 'predecessor_manifest_sha256', 'payload', 'update_revision'}
        require(set(self.new) == set(self.old) | {'update_revision'}, 'installation schema drift')
        require(all(self.new[k] == self.old[k] for k in self.old if k not in mutable), 'installation boundary drift')
        require(self.new['artifact_ref'] == spec['compiled_source_ref'] == '4668b424a58a0bb3e0083380ceb33c0d8312b76d'
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
        require(self.new['update_revision'] == 'sparse-4668b42-after-failed-6-v1', 'revision drift')
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
            actual, expected = inventory(self.path(name)), self.expected[key]
            if key == 'config_inventory':
                # Only these three public inputs may change, with identical
                # paths/custody. verify_files checks exact old/new payload bytes.
                def nonpayload(value):
                    import copy
                    result = copy.deepcopy(value)
                    for target in CHANGES[3:6]:
                        item = result[str(Path(target).relative_to('/etc/moex-finam-p1-paper'))]
                        item.pop('sha256')
                        item.pop('size')
                    return result
                actual, expected = nonpayload(actual), nonpayload(expected)
            require(actual == expected, 'terminal/selector/materialization invariant drift: ' + key)
        head = strict_json(custody.file_bytes(self.root, custody.CONTROL + '/authority/history-head.json', 0o440, self.gid))
        require(head == self.expected['history_head'], 'terminal head changed')
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
        require(value['domain'] == 'o2-sparse-4668b42-v1'
                and value['old_sha256'] == OLD_SHA and value['new_sha256'] == self.spec['new_manifest_sha256']
                and value['state'] in {'PREPARED', 'APPLIED', 'ROLLING_BACK', 'ROLLED_BACK'}, 'journal identity/state')
        for i, name in enumerate(CHANGES):
            raw = custody.file_bytes(self.root, TRANSACTION + f'/before-{i}', 0o600, 0)
            require(custody.sha(raw) == self.spec['updates'][name]['old_sha256'], 'backup corruption')
        return value

    def write_journal(self, state):
        value = dict(domain='o2-sparse-4668b42-v1', old_sha256=OLD_SHA,
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
                    gid = self.gid if self.old['payload'].get(name, {}).get('group') == 'service' else 0
                    custody.atomic_write(self.root, name, raw, mode, gid)
            self.verify_files(desired)
            self.invariants()
            self.write_journal(terminal)
            return {'state': terminal, 'history_preserved': True, 'activation_performed': False,
                    'daemon_reload_performed': False, 'redis_contact': False, 'finam_contact': False}

def load_inputs():
    from stage8b_p1f_o2_sparse_install_package import load_predecessor, validate_prepared
    spec = strict_json(SPEC.read_bytes())
    _, expected = load_predecessor(ROOT / 'installation-sparse/retained-terminal.zip',
                                   ROOT / 'installation-sparse/prior-installation.zip')
    payload = {name: (ROOT / 'installation-sparse/payload' / Path(name).name).read_bytes() for name in CHANGES}
    validate_prepared(ROOT / 'installation-sparse')
    return spec, expected, payload

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('preflight', 'status', 'apply', 'resume', 'rollback'))
    parser.add_argument('--confirm')
    args = parser.parse_args()
    try:
        require(os.geteuid() == 0, 'root required')
        require(args.confirm == (None if args.action in ('preflight', 'status') else ROLLBACK if args.action == 'rollback' else CONFIRM), 'exact operational confirmation required')
        spec, expected, payload = load_inputs()
        print(json.dumps(Update(Path('/'), payload, spec, expected).transition('preflight' if args.action == 'status' else args.action), sort_keys=True))
    except (custody.Error, OSError, ValueError, KeyError, zipfile.BadZipFile) as error:
        print('o2-terminal-update: FAIL ' + str(error), file=sys.stderr)
        raise SystemExit(1)
