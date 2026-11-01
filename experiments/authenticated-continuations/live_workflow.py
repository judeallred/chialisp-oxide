#!/usr/bin/env python3
"""Fixed-provider Testnet11 adapter. WIP, no chain authentication or finality.

No funding/signing. Only an explicit submit flag can push a prepared
permissionless zero-fee bundle. Once a push starts it is never retried: missing
coins and absent mempool entries do not prove that a timed-out push failed.
Private directories and cooperating local writers are required.
"""
import argparse
import hashlib
import json
from pathlib import Path
import tempfile
import subprocess

import bounded_process
import prepare_step
import ledger_checks
import workflow as w

ORIGIN = 'https://testnet11.api.coinset.org'
ENDPOINTS = {'get_network_info', 'get_blockchain_state', 'get_coin_record_by_name',
             'get_coin_records_by_parent_ids', 'get_puzzle_and_solution', 'push_tx',
             'get_mempool_item_by_tx_id'}
SCOPE = 'fixed HTTPS provider assertions; no chain authentication or finality'
PROTOCOL = 'chia-consensus-0.36.1/Testnet11/pre-hard-fork-2'
MAX_BYTES = 1024*1024
MAX_PUSH_RESPONSE = 65536


class Inconclusive(ValueError):
    """Provider availability/snapshot uncertainty, never policy rejection."""


def stable_tip(transport, before):
    if tip(transport) != before:
        raise Inconclusive('tip changed; observation inconclusive')


def decode(raw):
    w.require(len(raw) <= MAX_BYTES, 'bounded provider/public intent JSON')
    depth = 0
    quoted = escaped = False
    for byte in raw:
        if quoted:
            if escaped: escaped = False
            elif byte == 92: escaped = True
            elif byte == 34: quoted = False
        elif byte == 34: quoted = True
        elif byte in (91, 123):
            depth += 1
            w.require(depth <= 32, 'JSON nesting bound')
        elif byte in (93, 125): depth -= 1
    def pairs(rows):
        result = {}
        for key, value in rows:
            w.require(key not in result, 'duplicate JSON key')
            result[key] = value
        return result
    def invalid(_): raise ValueError('nonfinite JSON number')
    return json.loads(raw, object_pairs_hook=pairs, parse_constant=invalid)


class CurlTransport:
    def __init__(self, receipt_directory=None):
        self.receipts = []
        self.receipt_directory = receipt_directory

    def call(self, endpoint, body):
        w.require(endpoint in ENDPOINTS, 'fixed endpoint allowlist')
        if endpoint == 'push_tx':
            w.require(self.receipt_directory is not None, 'push requires durable public response directory')
            self.receipt_directory.mkdir(mode=0o700, parents=False, exist_ok=True)
            w.require(self.receipt_directory.is_dir() and not self.receipt_directory.is_symlink(), 'private response directory')
        response_limit = MAX_PUSH_RESPONSE if endpoint == 'push_tx' else MAX_BYTES
        w.require(len(self.receipts) < 32, 'HTTP receipt count bound before request')
        payload = w.encode(body)
        w.require(len(payload) <= 512*1024, 'bounded public request')
        with tempfile.TemporaryDirectory(prefix='continuation-http-') as directory:
            path = Path(directory)/'request.json'
            path.write_bytes(payload)
            try:
                result = bounded_process.run(['/usr/bin/curl', '--disable', '--silent', '--show-error',
                    '--max-time', '20', '--connect-timeout', '5', '--max-redirs', '0',
                    '--proto', '=https', '--proto-redir', '=https', '--noproxy', '*',
                    '--max-filesize', str(response_limit), '--header', 'Content-Type: application/json',
                    '--data-binary', '@'+str(path), '--write-out', '\n%{http_code}',
                ORIGIN+'/'+endpoint], timeout=25, stdout_limit=response_limit+4,
                    stderr_limit=16384, check=True)
            except (OSError, RuntimeError, subprocess.SubprocessError) as error:
                raise Inconclusive('provider HTTP transport/deadline/size failure') from error
        raw, status = result.stdout.rsplit(b'\n', 1)
        self.receipts.append(dict(endpoint=endpoint, http_status=int(status), request_bytes=len(payload),
            request_sha256=hashlib.sha256(payload).hexdigest(), response_bytes=len(raw),
            response_sha256=hashlib.sha256(raw).hexdigest()))
        if endpoint == 'push_tx':
            # Record complete bounded bytes before interpreting an acknowledgement.
            # A transport/size failure has no invented complete-body receipt.
            record = dict(schema=1, scope=SCOPE, receipt=self.receipts[-1], response_hex=raw.hex())
            name = self.receipts[-1]['request_sha256']+'-'+self.receipts[-1]['response_sha256']+'.json'
            write_intent(self.receipt_directory/name, record)
        if status != b'200':
            raise Inconclusive('exact HTTP 200 required; redirects/retries forbidden')
        try:
            return decode(raw)
        except (ValueError, RecursionError, UnicodeError) as error:
            raise Inconclusive('invalid or bounded provider JSON response') from error


def begin(transport):
    if hasattr(transport, 'receipts'): transport.receipts.clear()


def receipts(transport):
    result = list(getattr(transport, 'receipts', []))
    w.require(len(result) <= 32, 'bounded per-operation receipts')
    for row in result:
        w.fields(row, 'endpoint http_status request_bytes request_sha256 response_bytes response_sha256')
        w.require(row['endpoint'] in ENDPOINTS and type(row['http_status']) is int and 100 <= row['http_status'] <= 599,
                  'receipt endpoint/status')
        w.require(type(row['request_bytes']) is int and 0 <= row['request_bytes'] <= 512*1024
                  and type(row['response_bytes']) is int and 0 <= row['response_bytes'] <= MAX_BYTES, 'receipt byte bounds')
        w.hex32(row['request_sha256']); w.hex32(row['response_sha256'])
    return result


def rpc(transport, endpoint, body):
    value = transport.call(endpoint, body)
    if not (type(value) is dict and value.get('success') is True and value.get('error') is None):
        raise Inconclusive('successful provider envelope required')
    return value


def network(transport):
    value = rpc(transport, 'get_network_info', {})
    w.require(value.get('network_name') == 'testnet11' and value.get('network_prefix') == 'txch',
              'explicit Testnet11 network assertion required')


def uint(value):
    w.require(type(value) is int and 0 <= value <= 0xffffffff, 'exact u32')
    return value


def normalized(value):
    w.require(type(value) is str, 'hex string')
    value = value.removeprefix('0x')
    w.hex32(value)
    return value


def tip(transport):
    value = rpc(transport, 'get_blockchain_state', {})['blockchain_state']
    if not (value['sync']['synced'] is True and value['sync']['sync_mode'] is False):
        raise Inconclusive('synced provider required')
    height = uint(value['peak']['height'])
    previous = uint(value['peak']['prev_transaction_block_height'])
    w.require(previous <= height, 'provider previous transaction block height relation')
    return dict(height=height, header_hash=normalized(value['peak']['header_hash']), previous_transaction_height=previous)


def record(value, peak):
    w.fields(value, 'coin confirmed_block_index spent_block_index spent coinbase timestamp')
    w.fields(value['coin'], 'parent_coin_info puzzle_hash amount')
    coin = w.coin(normalized(value['coin']['parent_coin_info']), normalized(value['coin']['puzzle_hash']), value['coin']['amount'])
    confirmed = uint(value['confirmed_block_index'])
    spent = uint(value['spent_block_index'])
    w.require(type(value['spent']) is bool and type(value['coinbase']) is bool and value['coinbase'] is False
              and type(value['timestamp']) is int and value['timestamp'] >= 0
              and 0 < confirmed <= peak['height'] and spent <= peak['height']
              and ((value['spent'] and 0 < spent and confirmed <= spent)
                   or (not value['spent'] and spent == 0)), 'consistent confirmed/spent record')
    return dict(coin=coin, confirmed=confirmed, spent=spent)


def fetch_coin(transport, coin, peak):
    value = rpc(transport, 'get_coin_record_by_name', {'name':'0x'+coin['coin_id']})
    raw = value.get('coin_record')
    if raw is None: return None
    result = record(raw, peak)
    w.require(result['coin'] == coin, 'exact requested coin record')
    return result


def observe_current(journal, transport):
    begin(transport)
    with w.locked(journal):
        state = w.load(journal)
        current = w.view(state)['current_coin']
        w.require(current is not None, 'recorded incomplete funding coin required')
        network(transport)
        before = tip(transport)
        observed = fetch_coin(transport, current, before)
        stable_tip(transport, before)
        return dict(scope=SCOPE, tip=before, coin=observed, journal_revision_token=w.token(state), http_receipts=receipts(transport))


def intent_path(journal):
    return journal.with_name(journal.name+'.intent.json')


def write_intent(path, value):
    # Same atomic protocol and postcommit distinction as the public journal,
    # with an explicit larger limit for the retained optional-off reveal.
    payload = w.encode(value)
    w.require(len(payload) <= MAX_BYTES, 'bounded intent')
    # write() bounds at its fixed 64KiB; large intents use a compact payload
    # containing only exact unsigned bundle fields (off mode may exceed it).
    import os
    fd, temporary = tempfile.mkstemp(prefix=path.name+'.', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(payload); stream.flush(); os.fsync(stream.fileno())
        os.replace(temporary, path)
        try:
            directory = os.open(path.parent, os.O_RDONLY)
            try: os.fsync(directory)
            finally: os.close(directory)
        except OSError as error:
            raise w.PostcommitError('intent replaced but durability uncertain; reload before any action') from error
    finally:
        if os.path.exists(temporary): os.unlink(temporary)


def validate_receipt_rows(rows):
    w.require(type(rows) is list, 'receipt list')
    class Recorded:
        receipts = rows
    receipts(Recorded())


def validate_confirmation(value, intent):
    w.fields(value, 'scope tip spent_height parent children actual_spend http_receipts')
    w.require(value['scope'] == SCOPE, 'confirmation scope')
    w.fields(value['tip'], 'height header_hash previous_transaction_height')
    peak = value['tip']
    uint(peak['height']); w.hex32(peak['header_hash'])
    w.require(uint(peak['previous_transaction_height']) <= peak['height'], 'confirmation tip previous height')
    spent = uint(value['spent_height'])
    parent = value['parent']
    w.fields(parent, 'coin confirmed spent')
    uint(parent['spent'])
    w.require(parent['coin'] == intent['prepared']['current_coin'] and 0 < uint(parent['confirmed']) <= spent == parent['spent'] <= peak['height'], 'confirmation parent height/identity')
    children = value['children']
    w.require(type(children) is list and len(children) == len(intent['prepared']['children']), 'confirmation complete children')
    for row in children:
        w.fields(row, 'coin confirmed spent')
        uint(row['spent'])
        w.require(type(row['confirmed']) is int and row['confirmed'] == spent and (row['spent'] == 0 or spent <= uint(row['spent']) <= peak['height']), 'confirmation child creation/spend heights')
    w.require(sorted([row['coin'] for row in children], key=lambda row:row['coin_id']) == sorted(intent['prepared']['children'], key=lambda row:row['coin_id']), 'confirmation exact child identities')
    w.require(value['actual_spend'] == intent['prepared']['spend_bundle']['coin_spends'][0], 'confirmation actual reveal/solution')
    validate_receipt_rows(value['http_receipts'])
    w.require(all(row['http_status'] == 200 for row in value['http_receipts']), 'confirmation requires successful HTTP reads')
    w.require([row['endpoint'] for row in value['http_receipts']] == ['get_network_info', 'get_blockchain_state', 'get_coin_record_by_name', 'get_puzzle_and_solution', 'get_coin_records_by_parent_ids', 'get_blockchain_state'], 'confirmation complete provider read set')


def check_intent(value):
    w.fields(value, 'schema scope status original_state revision_token prepared confirmation http_receipts')
    w.require(value['schema'] == 1 and type(value['schema']) is int and value['scope'] == SCOPE
              and value['status'] in ('prepared', 'push_started', 'submitted', 'ambiguous', 'confirmed'), 'fixed intent schema/status')
    validate_receipt_rows(value['http_receipts'])
    state = value['original_state']
    artifact = w.selected(state['plan']['artifact']['mode'])
    w.validate(state, artifact)
    w.require(w.token(state) == value['revision_token'], 'intent original revision binding')
    report = value['prepared']
    w.fields(report, 'current_coin children spend_bundle spend_bundle_hex spend_bundle_sha256 spend_bundle_id measurement measurement_protocol host_executable_sha256')
    w.require(report['measurement_protocol'] == PROTOCOL, 'fixed measurement protocol')
    w.hex32(report['host_executable_sha256'])
    height = uint(report['measurement']['previous_transaction_height'])
    w.require(height < 0xffffffff, 'supported pre-hard-fork-2 height')
    ledger_checks.costs(report['measurement'], height)
    current = w.view(state)['current_coin']
    w.require(current is not None and report['current_coin'] == current, 'intent rooted current coin')
    index = state['completed']
    children = [w.coin(current['coin_id'], state['plan']['steps'][index]['recipient'], 1000)]
    if index+1 < len(state['plan']['steps']): children.append(w.expected_coin(state, index+1))
    w.require(report['children'] == children, 'intent exact expected children')
    bundle = report['spend_bundle']
    w.fields(bundle, 'coin_spends aggregated_signature')
    w.require(type(bundle['coin_spends']) is list and len(bundle['coin_spends']) == 1
              and bundle['aggregated_signature'] == 'c0'+'00'*95, 'one permissionless unsigned spend')
    spend = bundle['coin_spends'][0]
    w.fields(spend, 'coin puzzle_reveal solution')
    w.require(spend['coin'] == dict(parent_coin_info=current['parent_id'], puzzle_hash=current['puzzle_hash'], amount=current['amount']), 'exact spend coin')
    reveal = bytes.fromhex(spend['puzzle_reveal']); solution = bytes.fromhex(spend['solution'])
    w.require(reveal.hex() == spend['puzzle_reveal'] and solution.hex() == spend['solution'], 'canonical persisted program hex')
    raw = w.check_retained.a.raw_tree
    w.require(len(reveal) <= 100_000 and raw.tree_hash(raw.deserialize(reveal)).hex() == current['puzzle_hash'], 'exact committed reveal')
    row = state['plan']['steps'][index]
    expected_solution = raw.serialize(w.o.proper(w.o.proper(bytes.fromhex(row['recipient']), bytes.fromhex(row['next_digest']))))
    w.require(solution == expected_solution, 'exact committed opening solution')
    wire = b'\0\0\0\1'+bytes.fromhex(current['parent_id'])+bytes.fromhex(current['puzzle_hash'])+current['amount'].to_bytes(8,'big')+reveal+solution+bytes.fromhex(bundle['aggregated_signature'])
    digest = hashlib.sha256(wire).hexdigest()
    w.require(wire.hex() == report['spend_bundle_hex'] and digest == report['spend_bundle_sha256'] == report['spend_bundle_id'], 'exact persisted wire/bundle identity')
    if value['status'] == 'confirmed':
        validate_confirmation(value['confirmation'], value)
    else:
        w.require(value['confirmation'] is None, 'only confirmed intents carry confirmation')
    return value


def read_intent(journal):
    return check_intent(decode(w.payload(intent_path(journal), MAX_BYTES)))


def prepare(journal, executable, height, expected):
    with w.locked(journal):
        state = w.load(journal)
        w.require(w.token(state) == expected, 'stale journal revision')
        path = intent_path(journal)
        if path.exists(): w.require(read_intent(journal)['status'] == 'confirmed', 'prior intent must be confirmed')
        report = prepare_step.prepare(journal, executable, height)
        keys = 'current_coin children spend_bundle spend_bundle_hex spend_bundle_sha256 spend_bundle_id'.split()
        selected_report = {key:report[key] for key in keys}
        selected_report.update(measurement=report['costs'], measurement_protocol=PROTOCOL,
                               host_executable_sha256=report['host_executable_sha256'])
        intent = dict(schema=1, scope=SCOPE, status='prepared', original_state=state,
                      revision_token=expected, prepared=selected_report, confirmation=None, http_receipts=[])
        check_intent(intent)
        write_intent(path, intent)
        return intent


def confirmed_observation(transport, intent):
    network(transport)
    before = tip(transport)
    prepared = intent['prepared']
    current = fetch_coin(transport, prepared['current_coin'], before)
    if current is None or current['spent'] == 0:
        stable_tip(transport, before)
        return None
    spent = current['spent']
    actual = rpc(transport, 'get_puzzle_and_solution', {'coin_id':'0x'+prepared['current_coin']['coin_id'], 'height':spent})['coin_solution']
    expected = prepared['spend_bundle']['coin_spends'][0]
    w.require(type(actual) is dict, 'actual coin spend object')
    actual_coin = actual['coin']
    w.fields(actual_coin, 'parent_coin_info puzzle_hash amount')
    normalized_coin = dict(parent_coin_info=normalized(actual_coin['parent_coin_info']), puzzle_hash=normalized(actual_coin['puzzle_hash']), amount=actual_coin['amount'])
    w.require(normalized_coin == expected['coin']
              and actual['puzzle_reveal'].removeprefix('0x') == expected['puzzle_reveal']
              and actual['solution'].removeprefix('0x') == expected['solution'], 'actual spent reveal/solution differs')
    response = rpc(transport, 'get_coin_records_by_parent_ids', {'parent_ids':['0x'+prepared['current_coin']['coin_id']], 'include_spent_coins':True})
    rows = response['coin_records']
    w.require(type(rows) is list and len(rows) == len(prepared['children']) and not response.get('truncated', False), 'complete bounded child set')
    children = [record(row, before) for row in rows]
    w.require(all(row['confirmed'] == spent for row in children)
              and sorted([row['coin'] for row in children], key=lambda row:row['coin_id']) == sorted(prepared['children'], key=lambda row:row['coin_id']), 'exact confirmed full children (later spends allowed)')
    stable_tip(transport, before)
    return dict(scope=SCOPE, tip=before, spent_height=spent, parent=current, children=children,
                actual_spend=expected, http_receipts=receipts(transport))


def reconcile_locked(journal, transport, intent):
    observation = confirmed_observation(transport, intent)
    if observation is None: return intent
    state = w.load(journal)
    original = intent['original_state']
    advanced = json.loads(json.dumps(original))
    advanced['completed'] += 1; advanced['revision'] += 1
    w.validate(advanced, original['plan']['artifact'])
    if state == original: w.write(journal, advanced)
    else: w.require(state == advanced, 'journal changed beyond exact reconciliation step')
    intent['status'] = 'confirmed'
    intent['confirmation'] = observation
    check_intent(intent)
    write_intent(intent_path(journal), intent)
    return intent


def reconcile(journal, transport):
    begin(transport)
    with w.locked(journal):
        return reconcile_locked(journal, transport, read_intent(journal))


def rpc_bundle(bundle):
    """RPC hex presentation only; stored streamable bytes and bundle ID stay fixed."""
    value = json.loads(json.dumps(bundle))
    for spend in value['coin_spends']:
        for key in ('parent_coin_info', 'puzzle_hash'):
            spend['coin'][key] = '0x'+spend['coin'][key]
        for key in ('puzzle_reveal', 'solution'):
            spend[key] = '0x'+spend[key]
    value['aggregated_signature'] = '0x'+value['aggregated_signature']
    return value


def submit(journal, transport, expected, *, explicitly_submit=False, executable=None):
    begin(transport)
    w.require(explicitly_submit is True, 'explicit submit flag required')
    w.require(executable is not None, 'submit requires explicit local preflight executable')
    with w.locked(journal):
        state = w.load(journal)
        w.require(w.token(state) == expected, 'stale journal revision')
        intent = read_intent(journal)
        w.require(intent['revision_token'] == expected, 'prepared intent revision differs')
        # Reconcile before every possible POST; any previous attempt forbids
        # another push even when the provider reports no coin/mempool entry.
        intent = reconcile_locked(journal, transport, intent)
        if intent['status'] == 'confirmed': return intent
        w.require(intent['status'] == 'prepared', 'push already attempted; reconcile only, never blind retry')
        network(transport)
        peak = tip(transport)
        current = fetch_coin(transport, intent['prepared']['current_coin'], peak)
        if current is None:
            raise Inconclusive('funding coin not observed; no push attempted')
        w.require(current['spent'] == 0, 'input is already spent; reconcile required')
        stable_tip(transport, peak)
        w.require(peak['height'] < 0xffffffff, 'unsupported observed protocol height')
        fresh = prepare_step.prepare(journal, executable, peak['previous_transaction_height'])
        w.require(fresh['spend_bundle_hex'] == intent['prepared']['spend_bundle_hex'], 'fresh exact spend differs from persisted intent')
        stable_tip(transport, peak)
        intent['prepared'].update(measurement=fresh['costs'], measurement_protocol=PROTOCOL,
                                  host_executable_sha256=fresh['host_executable_sha256'])
        w.require(w.token(w.load(journal)) == expected, 'journal changed before push')
        intent['status'] = 'push_started'
        intent['http_receipts'] = receipts(transport)
        write_intent(intent_path(journal), intent)
        try:
            response = transport.call('push_tx', {'spend_bundle':rpc_bundle(intent['prepared']['spend_bundle'])})
            accepted = type(response) is dict and response.get('success') is True and response.get('status') == 'SUCCESS' and response.get('error') is None
            intent['status'] = 'submitted' if accepted else 'ambiguous'
            intent['http_receipts'] = receipts(transport)
        except Exception:
            intent['status'] = 'ambiguous'
            intent['http_receipts'] = receipts(transport)
            write_intent(intent_path(journal), intent)
            raise
        write_intent(intent_path(journal), intent)
        return intent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('observe', 'prepare', 'submit', 'reconcile'))
    parser.add_argument('--journal', type=Path, required=True)
    parser.add_argument('--executable', type=Path)
    parser.add_argument('--previous-transaction-height', type=int)
    parser.add_argument('--revision-token')
    parser.add_argument('--submit', action='store_true')
    args = parser.parse_args()
    if args.command == 'prepare':
        w.require(args.executable is not None and args.previous_transaction_height is not None, 'prepare requires executable and supplied height')
        result = prepare(args.journal, args.executable, args.previous_transaction_height, args.revision_token)
    elif args.command == 'observe': result = observe_current(args.journal, CurlTransport())
    elif args.command == 'reconcile': result = reconcile(args.journal, CurlTransport())
    else: result = submit(args.journal, CurlTransport(args.journal.with_name(args.journal.name+'.responses')), args.revision_token, explicitly_submit=args.submit, executable=args.executable)
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
