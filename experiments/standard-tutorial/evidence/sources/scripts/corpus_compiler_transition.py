"""Reviewed optimized-artifact transitions for retained rows of the generic corpus append root.

A compiler change may alter the optimized Rust CLVM of a retained export. The
profile declares which retained slugs it touches and why; this module admits
only that mode's artifact, its recorded VM costs and the fields of the dedicated
protocol build that derive from the module bytes, which the component validator
recomputes from the new artifact. Every other field of the row, every other
build, every typed outcome and failure, the unoptimized Rust artifact and all
independent-language artifacts must stay byte-identical. The checkpoint row is
then restored for pure historical projection. Finite agreement after such a
transition is regression evidence, not a compiler-preservation proof.
"""
import statistics

import p2_delegated_conditions_evidence as ledger

PROTOCOL = 'chialisp-oxide-corpus-compiler-transition/v1'
OPTIMIZED = ('rust_mir', True)
MODULE_DERIVED = ('module_hex', 'module_tree_hash', 'owner_curried_reveal_hex', 'owner_instance_hash', 'observations')
TRANSACTION_DERIVED = ('transaction', 'signature_source', 'child', 'alternate_build', 'selected_reveal_signature_source')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def mode(row):
    return (row['backend'], row['optimize_lir'])


def validate_declaration(value, preserved):
    ledger.closed(value, ['protocol', 'reason', 'slugs', 'metrics'], 'compiler transition')
    require(value['protocol'] == PROTOCOL and type(value['reason']) is str and len(value['reason']) >= 40, 'compiler transition protocol/reason')
    slugs = value['slugs']
    require(type(slugs) is list and slugs and len(set(slugs)) == len(slugs) and all(type(s) is str and s in preserved for s in slugs),
            'compiler transition slugs must be unique retained exports')
    metrics = value['metrics']
    if metrics is not None:
        require(type(metrics) is list and [row.get('slug') for row in metrics] == slugs, 'compiler transition metrics must cover its slugs in order')
        for row in metrics:
            ledger.closed(row, ['slug', 'before', 'after'], 'transition metric')
            for side in ('before', 'after'):
                ledger.closed(row[side], ['bytes', 'tree_hash', 'mean_accepted_cost'], 'transition measurement')
                require(type(row[side]['bytes']) is int and row[side]['bytes'] > 0 and ledger.hex_bytes(row[side]['tree_hash'], 32)
                        and (row[side]['mean_accepted_cost'] is None or type(row[side]['mean_accepted_cost']) is int), 'transition measurement types')
    return value


def _execution(old, new, label):
    ledger.closed(new, list(old), f'{label} execution')
    require({k: v for k, v in old.items() if k != 'cost'} == {k: v for k, v in new.items() if k != 'cost'},
            f'{label}: optimized Rust outcome/error/failure must be unchanged')
    if old['cost'] is None:
        require(new['cost'] is None, f'{label}: cost presence changed')
    else:
        require(type(new['cost']) is int and new['cost'] > 0, f'{label}: optimized cost must be an actual positive measurement')


def _runs(old, new, label):
    require(type(old) is list and type(new) is list and len(old) == len(new), f'{label}: run count')
    for a, b in zip(old, new, strict=True):
        ledger.closed(b, list(a), f'{label} run')
        require(mode(a) == mode(b), f'{label}: run mode/order changed')
        if mode(a) == OPTIMIZED:
            _execution(a['execution'], b['execution'], label)
        else:
            require(a == b, f'{label}: non-transitioned build changed')


def _cases(old, new, label):
    require(type(old) is list and type(new) is list and len(old) == len(new), f'{label}: case count')
    for a, b in zip(old, new, strict=True):
        ledger.closed(b, list(a), f'{label} case')
        require({k: v for k, v in a.items() if k != 'runs'} == {k: v for k, v in b.items() if k != 'runs'},
                f'{label}: case label/input/native/declared outcome changed')
        _runs(a['runs'], b['runs'], f'{label} {a.get("label", "")!r}')


def _artifacts(old, new):
    require(type(old) is list and type(new) is list and len(old) == len(new) == 5, 'five artifacts required')
    for a, b in zip(old, new, strict=True):
        ledger.closed(b, list(a), 'artifact')
        require(mode(a) == mode(b), 'artifact mode/order changed')
        if mode(a) == OPTIMIZED:
            code = ledger.hex_bytes(b['clvm_hex'])
            require(type(b['bytes']) is int and b['bytes'] == len(code) > 0 and type(b['disassembly']) is str and b['disassembly']
                    and ledger.sha_tree(ledger.tree(b['clvm_hex'])).hex() == b['tree_hash'], 'transitioned artifact metadata')
        else:
            require(a == b, 'unoptimized Rust and independent-language artifacts must be byte-identical')


def _protocol(old, new):
    if old is None:
        require(new is None, 'protocol evidence appeared')
        return
    ledger.closed(new, list(old), 'protocol')
    require({k: v for k, v in old.items() if k not in ('builds', 'raw_validation')}
            == {k: v for k, v in new.items() if k not in ('builds', 'raw_validation')}, 'protocol header/counts changed')
    raw_old, raw_new = old['raw_validation'], new['raw_validation']
    ledger.closed(raw_new, list(raw_old), 'raw validation')
    require({k: v for k, v in raw_old.items() if k != 'cases'} == {k: v for k, v in raw_new.items() if k != 'cases'}, 'raw suite header changed')
    _cases(raw_old['cases'], raw_new['cases'], 'raw case')
    require(type(old['builds']) is list and type(new['builds']) is list and len(old['builds']) == len(new['builds']) == 5, 'five protocol builds')
    for a, b in zip(old['builds'], new['builds'], strict=True):
        ledger.closed(b, list(a), 'protocol build')
        require(mode(a) == mode(b), 'protocol build mode/order changed')
        if mode(a) != OPTIMIZED:
            require(a == b, 'non-transitioned protocol build changed')
            continue
        require({k: v for k, v in a.items() if k not in MODULE_DERIVED} == {k: v for k, v in b.items() if k not in MODULE_DERIVED},
                'transitioned protocol build counts/keys/status changed')
        require(type(a['observations']) is list and type(b['observations']) is list and len(a['observations']) == len(b['observations']), 'observation count')
        for x, y in zip(a['observations'], b['observations'], strict=True):
            ledger.closed(y, list(x), 'observation')
            require({k: v for k, v in x.items() if k not in TRANSACTION_DERIVED} == {k: v for k, v in y.items() if k not in TRANSACTION_DERIVED},
                    'observation label/status/consensus code/equality flags changed')


def equivalent(old, new):
    """The new row may differ from the checkpoint row only in the optimized Rust artifact and what derives from it."""
    ledger.closed(new, list(old), 'transitioned row')
    skip = ('artifacts', 'cases', 'resource_controls', 'protocol_validation')
    require({k: v for k, v in old.items() if k not in skip} == {k: v for k, v in new.items() if k not in skip},
            f"{old['slug']}: row source/ABI/native/provenance/ledger/identity metadata changed")
    _artifacts(old['artifacts'], new['artifacts'])
    _cases(old['cases'], new['cases'], 'named case')
    require(('resource_controls' in old) == ('resource_controls' in new), 'resource controls presence changed')
    if 'resource_controls' in old:
        ledger.closed(new['resource_controls'], list(old['resource_controls']), 'resource controls')
        require(old['resource_controls']['max_cost'] == new['resource_controls']['max_cost'], 'resource budget changed')
        _cases(old['resource_controls']['cases'], new['resource_controls']['cases'], 'resource control')
    _protocol(old.get('protocol_validation'), new.get('protocol_validation'))


def measure(row):
    artifact = next(a for a in row['artifacts'] if mode(a) == OPTIMIZED)
    costs = [run['execution']['cost'] for case in row['cases'] if case['native']['status'] == 'accepted'
             for run in case['runs'] if mode(run) == OPTIMIZED and type(run['execution']['cost']) is int]
    return dict(bytes=artifact['bytes'], tree_hash=artifact['tree_hash'],
                mean_accepted_cost=round(statistics.mean(costs)) if costs else None)


def metrics(before, current, slugs):
    old = {row['slug']: row for row in before['puzzles']}
    new = {row['slug']: row for row in current['puzzles']}
    return [dict(slug=slug, before=measure(old[slug]), after=measure(new[slug])) for slug in slugs]
