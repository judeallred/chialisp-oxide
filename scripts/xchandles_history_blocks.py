"""Offline full-block provider observations, deliberately not CLVM artifacts.

Generators (including backreferences), signatures, headers and proofs remain
opaque server-observed fields. JSON/transport bounds and cross-record relations
are checked; no generator tree decoding/hashing, BLS or consensus replay occurs.
"""
import re
import xchandles_history as core

BASE = 'blocks/'
REQUEST_FIELDS = ('index', 'url', 'request', 'maximum_body_bytes', 'socket_timeout_seconds',
                  'hard_wall_seconds', 'redirects', 'proxies', 'retries', 'started_at')
OBSERVATION_FIELDS = (*REQUEST_FIELDS, 'raw_path', 'http_status', 'bytes', 'sha256',
                      'elapsed_seconds', 'observed_at', 'worker_exit_code', 'error')
COVERAGE_FIELDS = ('protocol', 'status', 'error', 'observed_at', 'source_inputs_unchanged',
    'completed_heights', 'request_count', 'maximum_requests', 'maximum_retained_bytes',
    'retained_bytes_before_final_summary', 'requests', 'observations',
    'total_observed_full_block_cost', 'total_observed_full_block_fees_decimal', 'scope')
ROW_FIELDS = ('height', 'reported_header_hash', 'full_block_reported_header_hash',
    'header_relation', 'reported_previous_hash', 'timestamp', 'is_transaction_block',
    'full_block_cost', 'full_block_fees_decimal', 'record_fees_decimal',
    'transaction_generator_present', 'transaction_generator_reported_bytes',
    'transaction_generator_reference_count', 'reported_aggregate_signature_bytes',
    'block_record_request_index', 'full_block_request_index', 'scope')
BLOCK_FIELDS = {'challenge_chain_ip_proof', 'challenge_chain_sp_proof', 'finished_sub_slots',
    'foliage', 'foliage_transaction_block', 'infused_challenge_chain_ip_proof',
    'reward_chain_block', 'reward_chain_ip_proof', 'reward_chain_sp_proof',
    'transactions_generator', 'transactions_generator_ref_list', 'transactions_info'}
SCOPE = ('129 whole-block HTTPS provider observations, including unrelated transactions. '
    'Record header hashes are bound to get_block requests, not recomputed/authenticated. '
    'Generators (possibly backreferenced), signatures and proofs are opaque data; '
    'no CLVM decoding/tree hashing, execution, consensus replay, bundle fee/cost attribution, '
    'inclusion/finality or historical binary authentication.')



def same(left, right):
    """Strict bounded JSON equality; bools never alias integers."""
    pending = [(left, right)]
    while pending:
        a, b = pending.pop()
        if type(a) is not type(b):
            return False
        if type(a) is dict:
            if a.keys() != b.keys():
                return False
            pending.extend((a[k], b[k]) for k in a)
        elif type(a) is list:
            if len(a) != len(b):
                return False
            pending.extend(zip(a, b))
        elif a != b:
            return False
    return True


def uint(value, maximum=2**64-1):
    return core.integer(value, maximum)


def hash32(value):
    core.require(type(value) is str and re.fullmatch('0x[0-9a-f]{64}', value), 'reported32byte hash')
    return value


def decimal(value):
    core.require(type(value) is str and re.fullmatch('0|[1-9][0-9]*', value)
                 and len(value) <= 20, 'unsigned decimal fee string')
    uint(int(value))
    return value


def request(packet, index, endpoint, body):
    stem = BASE+f'http/{index:03}-{endpoint}'
    initial = packet.json(stem+'.request.json')
    observation = packet.json(stem+'.observation.json')
    core.closed(initial, REQUEST_FIELDS, 'block request')
    core.closed(observation, OBSERVATION_FIELDS, 'block HTTP observation')
    core.require(same({k: observation[k] for k in REQUEST_FIELDS}, initial), 'initial request/observation binding')
    core.require(uint(initial['index'], 257) == index and initial['url'] == 'https://api.coinset.org/'+endpoint
                 and same(initial['request'], body), 'exact fixed-origin block request')
    # Equality does not type-check bool/int; inspect request numeric roles too.
    if endpoint == 'get_block_record_by_height':
        core.closed(initial['request'], ('height',), 'block height request')
        uint(initial['request']['height'])
    else:
        core.closed(initial['request'], ('header_hash',), 'block header request')
        hash32(initial['request']['header_hash'])
    cap = 65536 if endpoint == 'get_block_record_by_height' else core.MAX_MEMBER
    for key, expected in (('maximum_body_bytes', cap), ('socket_timeout_seconds', 15),
                          ('hard_wall_seconds', 25), ('retries', 0)):
        core.require(uint(initial[key]) == expected, 'exact block transport bound')
    core.require(initial['redirects'] is False and initial['proxies'] is False
                 and type(initial['started_at']) is str, 'closed block transport flags')
    core.require(uint(observation['http_status']) == 200 and uint(observation['worker_exit_code']) == 0
                 and observation['error'] is None, 'conclusive HTTP observation')
    elapsed = observation['elapsed_seconds']
    core.require(type(elapsed) in (int, float) and 0 <= elapsed <= 25, 'recorded HTTP elapsed bound')
    name = stem+'.raw.json'
    core.require(observation['raw_path'] == name.removeprefix(BASE), 'raw response relative path')
    raw = packet.raw(name)
    core.require(uint(observation['bytes'], cap) == len(raw)
                 and core.ident(raw) == {k: observation[k] for k in ('bytes', 'sha256')}, 'block raw byte identity')
    value = packet.json(name)
    key = 'block_record' if endpoint == 'get_block_record_by_height' else 'block'
    core.closed(value, ('success', key), 'block RPC response')
    core.require(value['success'] is True and type(value[key]) is dict, 'strict block provider success')
    return value[key], observation, {stem+'.request.json', stem+'.observation.json', name}


def verify(packet, core_receipt):
    coverage = packet.json(BASE+'coverage-and-scope.json')
    core.closed(coverage, COVERAGE_FIELDS, 'block coverage')
    core.require(coverage['protocol'] == 'chialisp-oxide-xchandles15-full-block-observations/v1'
        and coverage['status'] == 'all129_full_block_observations_collected'
        and coverage['error'] is None and coverage['source_inputs_unchanged'] is True, 'complete block observation status')
    for key, expected in (('completed_heights', 129), ('request_count', 258),
                          ('maximum_requests', 260), ('maximum_retained_bytes', 48*1024*1024)):
        core.require(uint(coverage[key]) == expected, 'block coverage count/bound')
    uint(coverage['retained_bytes_before_final_summary'], 48*1024*1024)
    core.require(type(coverage['requests']) is list and len(coverage['requests']) == 258
                 and type(coverage['observations']) is list and len(coverage['observations']) == 129,
                 '258 block RPCs and129 rows')
    before = packet.json(BASE+'source-inputs-before.json')
    core.require(type(before) is list and len(before) == 3
                 and same(before, packet.json(BASE+'source-inputs-after.json')), 'recorded block source window')
    for row in before:
        core.closed(row, ('path', 'bytes', 'sha256'), 'opaque block source descriptor')
        core.require(type(row['path']) is str and type(row['sha256']) is str
                     and re.fullmatch('[0-9a-f]{64}', row['sha256']), 'block source descriptor')
        uint(row['bytes'], core.MAX_MEMBER)
    core.require(len({x['path'] for x in before}) == 3, 'unique block source descriptors')
    descriptor = {'path': 'target/xchandles15-research-20261026/attempt4/execution-receipt.json',
                  **core.ident(packet.raw('core/attempt4/execution-receipt.json'))}
    core.require(same(before[1], descriptor), 'block source receipt/core binding')
    heights = sorted(core.integer(s['record']['spent_block_index']) for s in core_receipt['steps'])
    core.require(len(heights) == len(set(heights)) == 129, '129 distinct core heights')
    requested = packet.json(BASE+'requested-heights.json')
    core.closed(requested, ('heights', 'source', 'scope'), 'requested block heights')
    core.require(same(requested['heights'], heights) and all(type(x) is int for x in requested['heights'])
                 and same(requested['source'], descriptor), 'exact requested frontier')
    expected = {BASE+n for n in ('coverage-and-scope.json', 'source-inputs-before.json',
                                'source-inputs-after.json', 'requested-heights.json')}
    rows, fees, costs = [], [], []
    for index, height in enumerate(heights):
        rec, rq, names = request(packet, 2*index, 'get_block_record_by_height', {'height': height})
        expected.update(names)
        core.require(uint(rec['height']) == height, 'block record requested height')
        header, previous = hash32(rec['header_hash']), hash32(rec['prev_hash'])
        fee, timestamp = uint(rec['fees']), uint(rec['timestamp'])
        block, bq, names = request(packet, 2*index+1, 'get_block', {'header_hash': header})
        expected.update(names)
        core.require(BLOCK_FIELDS <= block.keys() <= BLOCK_FIELDS | {'header_hash'}, 'closed full block top fields')
        reward, foliage, info = block['reward_chain_block'], block['foliage_transaction_block'], block['transactions_info']
        core.require(type(reward) is dict and type(foliage) is dict and type(info) is dict
                     and uint(reward['height']) == height and reward['is_transaction_block'] is True,
                     'full block height/transaction flag')
        cost = uint(info['cost'], core.MAX_COST)
        core.require(uint(info['fees']) == fee and uint(foliage['timestamp']) == timestamp
                     and hash32(block['foliage']['prev_block_hash']) == previous, 'block record/body relations')
        reported = block.get('header_hash')
        if reported is not None:
            core.require(hash32(reported) == header, 'optional reported header equality')
        signature = info['aggregated_signature']
        core.require(type(signature) is str and re.fullmatch('0x[0-9a-f]{192}', signature), 'opaque signature encoding')
        generator, refs = block['transactions_generator'], block['transactions_generator_ref_list']
        core.require(generator is None or (type(generator) is str and len(generator) <= core.MAX_MEMBER
                     and re.fullmatch(r'0x(?:[0-9a-f]{2})*', generator)), 'opaque generator string bound')
        core.require(type(refs) is list and len(refs) <= 65536, 'opaque generator ref list')
        for ref in refs:
            uint(ref, 2**32-1)
        row = coverage['observations'][index]
        core.closed(row, ROW_FIELDS, 'per-height block observation')
        for key, expected_value in (('height', height), ('timestamp', timestamp), ('full_block_cost', cost),
                ('block_record_request_index', 2*index), ('full_block_request_index', 2*index+1),
                ('transaction_generator_reference_count', len(refs)), ('reported_aggregate_signature_bytes', 96)):
            core.require(uint(row[key]) == expected_value, 'typed block row relation')
        core.require(row['reported_header_hash'] == header and row['full_block_reported_header_hash'] == reported
            and row['reported_previous_hash'] == previous and row['is_transaction_block'] is True
            and decimal(row['full_block_fees_decimal']) == str(fee) == decimal(row['record_fees_decimal'])
            and row['transaction_generator_present'] is (generator is not None), 'block row fields/fees/opaque flags')
        size = None if generator is None else (len(generator)-2)//2
        core.require(row['transaction_generator_reported_bytes'] is None if size is None
                     else type(row['transaction_generator_reported_bytes']) is int
                     and row['transaction_generator_reported_bytes'] == size, 'opaque generator encoded size')
        name = BASE+f'height-{height}.json'
        core.require(same(packet.json(name), row), 'height receipt exact relation')
        expected.add(name)
        core.require(same(coverage['requests'][2*index:2*index+2], [rq, bq]), 'coverage raw HTTP observation relation')
        rows.append(dict(height=height, fullBlockCost=cost, fullBlockFeesDecimal=str(fee),
            timestamp=timestamp, headerReported=header, headerRecomputed=False,
            blockRecordDownload=BASE+rq['raw_path'], fullBlockDownload=BASE+bq['raw_path'],
            blockObservationDownload=name))
        fees.append(fee)
        costs.append(cost)
    core.require(uint(coverage['total_observed_full_block_cost']) == sum(costs)
                 and decimal(coverage['total_observed_full_block_fees_decimal']) == str(sum(fees)), 'block aggregate totals')
    # Closed historical observation profile, not authentication of these blocks.
    core.require(sum(costs) == 812452089354 and sum(fees) == 3341788643120, 'captured block observation totals')
    return dict(rows=rows, expected=expected, costSum=sum(costs), feesSumDecimal=str(sum(fees)), scope=SCOPE)
