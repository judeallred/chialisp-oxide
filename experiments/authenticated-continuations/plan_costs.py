#!/usr/bin/env python3
"""Bounded offline batch-one queue costs from actual isolated SDK preparations.

No network requests, funding, signing or submission. Each step inserts its public
coin into a fresh private simulator: this is not a mined ledger chain. Fees are
zero for these policy spends; funding and fee sponsorship are excluded.
"""
import argparse
import hashlib
import json
from pathlib import Path
import tempfile

import ledger_checks
import prepare_step
import workflow as w

MAX_REPORT_BYTES = 16*1024*1024
MAX_STEP_BYTES = 2*1024*1024+4096
MAX_METADATA_BYTES = 65536
SYNTHETIC_PARENT = hashlib.sha256(b'chialisp-oxide/continuation-cost-plan/synthetic-parent/v1').hexdigest()
SCOPE = 'offline batch-one measured queue; isolated supplied-coin SDK preflights, not a ledger chain or live observations'
PROTOCOL = 'chia-consensus-0.36.1/Testnet11/pre-hard-fork-2'
METRICS = ('charged_bytes', 'execution_cost', 'condition_cost', 'byte_cost', 'total_cost')


def executable_digest(path):
    before = path.stat()
    w.require(path.is_file() and before.st_size <= 256*1024*1024, 'bounded local executable')
    digest = hashlib.sha256()
    remaining = before.st_size
    with path.open('rb') as stream:
        while remaining:
            chunk = stream.read(min(65536, remaining))
            w.require(bool(chunk), 'executable truncated while hashing')
            remaining -= len(chunk)
            digest.update(chunk)
        w.require(not stream.read(1), 'executable grew while hashing')
    return digest.hexdigest()


def check_prepared(state, prepared, height, executable_hash):
    w.fields(prepared, 'schema scope network mode module_hash module_blob_sha256 completed current_coin children fees spend_bundle_hex spend_bundle_sha256 spend_bundle_id sdk_preflight costs spend_bundle journal_revision_token host_executable_sha256')
    artifact = state['plan']['artifact']
    index = state['completed']
    current = w.expected_coin(state, index)
    row = state['plan']['steps'][index]
    children = [w.coin(current['coin_id'], row['recipient'], 1000)]
    if index+1 < len(state['plan']['steps']):
        children.append(w.expected_coin(state, index+1))
    w.require(type(prepared['schema']) is int and prepared['schema'] == 1
              and prepared['network'] == 'testnet11' and prepared['mode'] == artifact['mode']
              and prepared['scope'] == 'offline supplied-public-coin preflight; no chain-state authentication or submission'
              and type(prepared['completed']) is int and prepared['completed'] == index
              and type(prepared['fees']) is int and prepared['fees'] == 0,
              'fixed isolated preparation scope and step')
    w.require(prepared['module_hash'] == artifact['module_hash']
              and prepared['module_blob_sha256'] == artifact['blob_sha256']
              and prepared['current_coin'] == current and prepared['children'] == children
              and prepared['journal_revision_token'] == w.token(state)
              and prepared['host_executable_sha256'] == executable_hash,
              'exact artifact, executable, current coin, outputs and revision')
    sdk = prepared['sdk_preflight']
    w.fields(sdk, 'accepted inserted_public_coin simulator_previous_height')
    w.require(sdk['accepted'] is True and sdk['inserted_public_coin'] is True
              and type(sdk['simulator_previous_height']) is int and sdk['simulator_previous_height'] == 0,
              'explicit fresh isolated simulator preflight')
    bundle = prepared['spend_bundle']
    w.fields(bundle, 'coin_spends aggregated_signature')
    w.require(type(bundle['coin_spends']) is list and len(bundle['coin_spends']) == 1
              and bundle['aggregated_signature'] == 'c0'+'00'*95, 'one zero-fee permissionless spend')
    spend = bundle['coin_spends'][0]
    w.fields(spend, 'coin puzzle_reveal solution')
    w.require(spend['coin'] == dict(parent_coin_info=current['parent_id'], puzzle_hash=current['puzzle_hash'], amount=current['amount']), 'exact prepared spend coin')
    raw_tree = w.check_retained.a.raw_tree
    reveal = bytes.fromhex(spend['puzzle_reveal'])
    solution = bytes.fromhex(spend['solution'])
    w.require(len(reveal) <= 100000 and reveal.hex() == spend['puzzle_reveal']
              and raw_tree.serialize(raw_tree.deserialize(reveal)) == reveal
              and raw_tree.tree_hash(raw_tree.deserialize(reveal)).hex() == current['puzzle_hash'], 'canonical committed reveal')
    expected_solution = raw_tree.serialize(w.o.proper(w.o.proper(bytes.fromhex(row['recipient']), bytes.fromhex(row['next_digest']))))
    w.require(solution == expected_solution and solution.hex() == spend['solution'], 'exact queue opening')
    wire = b'\0\0\0\1'+bytes.fromhex(current['parent_id'])+bytes.fromhex(current['puzzle_hash'])+current['amount'].to_bytes(8,'big')+reveal+solution+bytes.fromhex(bundle['aggregated_signature'])
    w.require(wire.hex() == prepared['spend_bundle_hex']
              and hashlib.sha256(wire).hexdigest() == prepared['spend_bundle_id'] == prepared['spend_bundle_sha256'], 'exact complete wire identity')
    w.require(type(prepared['costs']['previous_transaction_height']) is int, 'exact measured height integer')
    metrics = ledger_checks.costs(prepared['costs'], height)
    for values in metrics.values():
        w.require(all(type(values[name]) is int and values[name] >= 0 for name in METRICS), 'nonnegative integer cost metrics')
    return metrics


def totals(records):
    result = {}
    for encoding in ('uncompressed', 'backrefs'):
        rows = [record['metrics'][encoding] for record in records]
        result[encoding] = {name:sum(row[name] for row in rows) for name in METRICS}
        result[encoding]['maximum_step_total_cost'] = max(row['total_cost'] for row in rows)
    return result


def check_metric_projection(value, *, maximum=False):
    w.fields(value, 'uncompressed backrefs')
    names = (*METRICS, 'maximum_step_total_cost') if maximum else METRICS
    for row in value.values():
        w.fields(row, ' '.join(names))
        w.require(all(type(row[name]) is int and row[name] >= 0 for name in names),
                  'exact nonnegative integer projected costs')


def validate(report):
    w.fields(report, 'schema scope network protocol mode previous_transaction_height funding_context artifact host_executable_sha256 recipients payout_per_item_mojos policy_spend_fees_mojos excluded_costs isolated_sdk_preflights ledger_chain_runs live_network_requests steps totals')
    w.require(type(report['schema']) is int and report['schema'] == 1 and report['scope'] == SCOPE
              and report['network'] == 'testnet11' and report['protocol'] == PROTOCOL, 'fixed planner schema/scope/protocol')
    height = report['previous_transaction_height']
    w.require(type(height) is int and 0 <= height < 0xffffffff, 'supported supplied previous transaction height')
    artifact = w.selected(report['mode'])
    w.require(report['artifact'] == artifact, 'complete current-source retained artifact admission')
    recipients = report['recipients']
    plan = w.plan(artifact, recipients)
    context = report['funding_context']
    w.fields(context, 'kind parent_id')
    w.hex32(context['parent_id'])
    w.require(context['kind'] in ('synthetic_fixture_not_observed', 'caller_supplied_public_parent_not_observed'), 'unobserved funding context')
    if context['kind'] == 'synthetic_fixture_not_observed':
        w.require(context['parent_id'] == SYNTHETIC_PARENT, 'fixed synthetic fixture parent')
    w.hex32(report['host_executable_sha256'])
    w.require(type(report['payout_per_item_mojos']) is int and report['payout_per_item_mojos'] == 1000
              and type(report['policy_spend_fees_mojos']) is int and report['policy_spend_fees_mojos'] == 0
              and report['excluded_costs'] == ['funding', 'fee sponsorship', 'future network fees']
              and type(report['isolated_sdk_preflights']) is int and report['isolated_sdk_preflights'] == len(recipients)
              and type(report['ledger_chain_runs']) is int and report['ledger_chain_runs'] == 0
              and type(report['live_network_requests']) is int and report['live_network_requests'] == 0,
              'cost and execution scope')
    records = report['steps']
    w.require(type(records) is list and len(records) == len(recipients), 'complete bounded queue step inventory')
    state = dict(schema=1, scope=w.SCOPE, network='testnet11', revision=1, plan=plan,
                 funding_parent=context['parent_id'], completed=0)
    for index, record in enumerate(records):
        w.fields(record, 'index prepared metrics')
        w.require(type(record['index']) is int and record['index'] == index, 'ordered queue records')
        w.require(len(w.encode(record)) <= MAX_STEP_BYTES, 'bounded retained step trace')
        measured = check_prepared(state, record['prepared'], height, report['host_executable_sha256'])
        expected_metrics = {encoding:{name:values[name] for name in METRICS} for encoding,values in measured.items()}
        check_metric_projection(record['metrics'])
        w.require(record['metrics'] == expected_metrics, 'metrics derive from retained full generator trace')
        state['completed'] += 1
        state['revision'] += 1
        w.validate(state, artifact)
    check_metric_projection(report['totals'], maximum=True)
    w.require(report['totals'] == totals(records), 'whole queue cost sums and maximum step costs')
    w.require(len(w.encode(report)) <= MAX_REPORT_BYTES, 'aggregate planner report bound')
    return report


def plan_costs(recipients, mode, executable, previous_transaction_height, funding_parent=None):
    w.require(type(recipients) is list and 1 <= len(recipients) <= 8, 'public recipient count 1..8')
    for recipient in recipients:
        w.hex32(recipient)
    recipients = list(recipients)
    w.require(mode in ('on', 'off'), 'exact Rust mode')
    w.require(type(previous_transaction_height) is int and 0 <= previous_transaction_height < 0xffffffff,
              'supported supplied previous transaction height')
    parent = SYNTHETIC_PARENT if funding_parent is None else funding_parent
    w.hex32(parent)
    executable = executable.resolve(strict=True)
    executable_hash = executable_digest(executable)
    records = []
    remaining = MAX_REPORT_BYTES-MAX_METADATA_BYTES
    with tempfile.TemporaryDirectory(prefix='continuation-cost-plan-') as directory:
        journal = Path(directory)/'private-journal.json'
        state = w.initialize(journal, mode, recipients)
        state = w.mutate(journal, w.token(state), parent=parent)
        artifact = state['plan']['artifact']
        w.require(len(w.encode(dict(artifact=artifact, recipients=recipients))) <= MAX_METADATA_BYTES-4096,
                  'bounded queue metadata before preparation')
        for index in range(len(recipients)):
            w.require(remaining >= MAX_STEP_BYTES, 'aggregate trace capacity before next preparation')
            prepared = prepare_step.prepare(journal, executable, previous_transaction_height)
            measured = check_prepared(state, prepared, previous_transaction_height, executable_hash)
            record = dict(index=index, prepared=prepared,
                          metrics={encoding:{name:values[name] for name in METRICS} for encoding,values in measured.items()})
            size = len(w.encode(record))
            w.require(size <= MAX_STEP_BYTES, 'bounded preparation trace')
            remaining -= size
            records.append(record)
            state = w.mutate(journal, w.token(state), observation=dict(network='testnet11',
                coin_id=prepared['current_coin']['coin_id'], children=prepared['children']))
        w.require(state['completed'] == len(recipients) and w.view(state)['current_coin'] is None, 'complete private queue plan')
    report = dict(schema=1, scope=SCOPE, network='testnet11', protocol=PROTOCOL, mode=mode,
                  previous_transaction_height=previous_transaction_height,
                  funding_context=dict(kind='synthetic_fixture_not_observed' if funding_parent is None else 'caller_supplied_public_parent_not_observed', parent_id=parent),
                  artifact=artifact, host_executable_sha256=executable_hash, recipients=recipients,
                  payout_per_item_mojos=1000, policy_spend_fees_mojos=0,
                  excluded_costs=['funding', 'fee sponsorship', 'future network fees'],
                  isolated_sdk_preflights=len(recipients), ledger_chain_runs=0, live_network_requests=0,
                  steps=records, totals=totals(records))
    w.require(executable_digest(executable) == executable_hash, 'executable changed during complete queue planning')
    return validate(report)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--recipients', type=Path, required=True)
    parser.add_argument('--mode', choices=('on', 'off'), required=True)
    parser.add_argument('--executable', type=Path, required=True)
    parser.add_argument('--previous-transaction-height', type=int, required=True)
    parser.add_argument('--funding-parent')
    args = parser.parse_args()
    report = plan_costs(w.read(args.recipients), args.mode, args.executable,
                        args.previous_transaction_height, args.funding_parent)
    print(json.dumps(report, sort_keys=True))


if __name__ == '__main__':
    main()
