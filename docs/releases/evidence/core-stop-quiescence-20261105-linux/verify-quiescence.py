import hashlib, json, pathlib, shutil, sys, types
sys.dont_write_bytecode = True
root = pathlib.Path('/projects/miy-v2')
path = root / 'scripts/test-core-ops.py'
fixture = types.ModuleType('review_fixture')
fixture.__file__ = str(path)
exec(compile(path.read_text(), str(path), 'exec'), fixture.__dict__)
observations = []
def run_case(name, check):
    case = fixture.CoreRolloutTests('test_apply_continues_when_stopping_leaves_a_unit_failed')
    case.setUp()
    try:
        detail = check(case)
        observations.append({'case': name, 'status': 'PASS', **detail})
    finally:
        case.doCleanups()
def populated_stop(case):
    case.host.fail_on_stop = {'miy-v2-worker'}
    case.host.retain_processes = {'miy-v2-worker'}
    code, result, release = case.run_tool()
    record = json.loads((release / 'deployment.json').read_text())
    assert code == 1 and record['failureCategory'] == 'core_processes_remaining'
    assert all(event not in case.host.events for event in ('reset-failed', 'backup', 'verify', 'start'))
    assert case.installed.read_bytes() == fixture.OLD
    return {'reportedStatus': record['status'], 'failure': record['failureCategory'], 'events': case.host.events,
            'previousBinaryPreserved': True, 'cgroupStillPopulated': True}
def stopped_with_cleared_path(case):
    assert case.run_tool()[0] == 0
    release = case.rollout.REL
    case.host.states = {service: 'failed' for service in case.rollout.SERVICES}
    for service in case.rollout.SERVICES:
        case.host.population(service, service == 'miy-v2-worker')
        case.host.process_properties[service] = {'ControlGroup': ''}
    case.host.work = [{**fixture.ZERO_WORK, 'runtime': 1}]
    events = list(case.host.events)
    code, result, _ = case.run_tool('rollback', release=release)
    assert code == 1 and result['failure'] == 'core_processes_remaining'
    assert case.host.events == events and case.installed.read_bytes() == fixture.NEW
    return {'failure': result['failure'], 'newServiceEvents': [], 'clearedControlGroupDidNotHideResidual': True}
def empty_failed_recovery(case):
    assert case.run_tool()[0] == 0
    release = case.rollout.REL
    case.host.states = {service: 'failed' for service in case.rollout.SERVICES}
    for service in case.rollout.SERVICES:
        case.host.population(service, False)
        case.host.process_properties[service] = {'ControlGroup': ''}
    case.host.work = [{**fixture.ZERO_WORK, 'runtime': 1}]
    code, result, _ = case.run_tool('rollback', release=release)
    assert code == 0 and result['status'] == 'ROLLED_BACK'
    assert case.installed.read_bytes() == fixture.OLD
    return {'reportedStatus': result['status'], 'persistedWorkRecoveryRetained': True}
def repopulated_after_reset(case):
    case.host.fail_on_stop = {'miy-v2-worker'}
    def after_reset():
        case.host.retain_processes = {'miy-v2-worker'}
        case.host.population('miy-v2-worker', True)
    case.host.on_reset_failed = after_reset
    code, result, release = case.run_tool()
    record = json.loads((release / 'deployment.json').read_text())
    assert code == 1 and record['failureCategory'] == 'core_processes_remaining'
    assert 'reset-failed' in case.host.events
    assert all(event not in case.host.events for event in ('backup', 'verify', 'start'))
    assert case.installed.read_bytes() == fixture.OLD
    return {'failure': record['failureCategory'], 'events': case.host.events, 'previousBinaryPreserved': True}
def vanished_known_cgroups(case):
    case.host.states = {service: 'inactive' for service in case.rollout.SERVICES}
    for service in case.rollout.SERVICES:
        case.host.process_properties[service] = {'ControlGroup': ''}
        shutil.rmtree(case.rollout.CGROUP_ROOT / 'system.slice' / (service + '.service'))
    units = case.rollout.require_core_quiescent()
    assert len(units) == 2
    return {'verifiedAbsentCanonicalGroupsAccepted': True}
for name, check in [('populated_failed_stop_blocks_publication', populated_stop),
                    ('cleared_control_group_still_checks_canonical_path', stopped_with_cleared_path),
                    ('empty_failed_core_allows_manual_recovery', empty_failed_recovery),
                    ('population_after_reset_still_blocks_publication', repopulated_after_reset),
                    ('known_absent_cgroups_allow_recovery', vanished_known_cgroups)]:
    run_case(name, check)
print(json.dumps({'scope': 'temporary cgroup files and mocked systemd/DB/Docker only',
                  'sourceSha256': hashlib.sha256((root / 'scripts/core-rollout.py').read_bytes()).hexdigest(),
                  'status': 'PASS', 'observations': observations}, indent=2))
