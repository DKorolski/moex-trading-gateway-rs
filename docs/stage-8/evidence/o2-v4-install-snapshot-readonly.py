import datetime
import json
from pathlib import Path
import stage8b_p1f_o2_sparse_install as old

c = old.custody
root = Path('/')
service = 'moex-finam-p1-o2-once-20261005.service'
timer = 'moex-finam-p1-o2-once-20261005.timer'
schedule = '/root/moex-o2-once-20261005'
require = c.require
command = c.command
def properties(unit, keys):
    rows = [r.split('=', 1) for r in command(['systemctl', 'show', unit, '--no-pager', '--property='+','.join(keys)]).splitlines()]
    value = dict(rows)
    require(len(value) == len(rows) and set(value) == set(keys), 'properties inventory')
    return value
common = ['LoadState','ActiveState','SubState','FragmentPath','DropInPaths','NeedDaemonReload','UnitFileState','Job']
units = {timer: properties(timer, common + ['NextElapseUSecRealtime','NextElapseUSecMonotonic','TimersCalendar','Unit','Persistent']),
         service: properties(service, common + ['MainPID','ControlPID','ControlGroup','Restart','ExecMainStatus'])}
for name, data in units.items():
    require(data['LoadState']=='loaded' and data['UnitFileState']=='static' and data['Job']=='' and data['DropInPaths']=='' and data['NeedDaemonReload']=='no', 'scheduler loaded')
    require(data['FragmentPath']=='/etc/systemd/system/'+name, 'scheduler path')
require(units[timer]['ActiveState']=='active' and units[timer]['SubState']=='elapsed'
        and units[timer]['NextElapseUSecRealtime']=='' and units[timer]['NextElapseUSecMonotonic']=='infinity'
        and units[timer]['Persistent']=='no' and units[timer]['Unit']==service, 'timer not exhausted')
require(units[service]['ActiveState']=='failed' and units[service]['SubState']=='failed'
        and units[service]['MainPID']==units[service]['ControlPID']=='0' and units[service]['ControlGroup']==''
        and units[service]['Restart']=='no' and units[service]['ExecMainStatus']=='1', 'caller not terminal')
def filtered(args):
    text = command(args)
    if args[:2] in (['systemctl','list-units'], ['systemctl','list-unit-files']):
        return '\n'.join(r for r in text.splitlines() if not r.split() or r.split()[0] not in {timer,service})
    return text
c.command = filtered
observation = c.observe(root)
c.command = command
raw = c.file_bytes(root,c.MANIFEST,0o644,0)
manifest = old.strict_json(raw)
uid,gid = c.old.account_ids(root)
for name,entry in manifest['payload'].items():
    value = c.file_bytes(root,name,int(entry['mode'],8),gid if entry['group']=='service' else 0)
    require(c.sha(value)==entry['sha256'] and len(value)==entry['size'],'managed bytes')
expected = dict(observation=observation, history_head=old.strict_json(c.file_bytes(root,c.CONTROL+'/authority/history-head.json',0o440,gid)))
for name,key in [(c.CONTROL,'authority_inventory'),('/etc/moex-finam-p1-paper','config_inventory'),
                 ('/var/lib/moex-finam-p1-paper/state','durable_inventory'),(c.STAGING,'staging_inventory'),
                 (schedule,'exhausted_schedule_inventory'),(old.TRANSACTION,'prior_transaction_inventory')]:
    expected[key]=old.inventory(Path(name))
require(expected['history_head']['state']=='FAILED' and expected['history_head']['latest_sequence']==8 and expected['history_head']['authority_generation']==1,'terminal head')
for p in ['/run/moex-finam-p1f-o2-input','/run/credentials/moex-finam-p1f-o2-materializer.service',schedule+'/auth.private.json',*c.old.OPERATOR_FILES]:
    require(not c.exists(root,p),'unexpected operational input')
expected['exhausted_schedule_units']=units
expected['exhausted_schedule_unit_sha256']={n:c.sha(c.file_bytes(root,'/etc/systemd/system/'+n,0o644,0)) for n in [timer,service]}
print(json.dumps(dict(status='PASS',observed_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
    installation_sha256=c.sha(raw), installation_manifest_text=raw.decode(), expected=expected,
    redis_contact=False,finam_contact=False,secret_values_exported=False,mutation_performed=False),sort_keys=True))
