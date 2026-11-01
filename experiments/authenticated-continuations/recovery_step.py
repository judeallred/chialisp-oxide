#!/usr/bin/env python3
"""One explicit exact-bundle retry; absence is never proof a prior push failed.

The ordinary submit operation still forbids repeat pushes. This separate action
archives the original intent before authorizing at most one further submission
of identical bytes. Private directories and cooperating writers are required.
No funding, keys, changed fees or automatic retry are supported.
"""
import argparse
import os
from pathlib import Path
import tempfile

import live_workflow as live
import prepare_step
import workflow as w


def archive_path(journal, bundle_id):
    w.hex32(bundle_id)
    return journal.with_name(journal.name+'.retry-original-'+bundle_id+'.json')


def archive_once(path, payload):
    """Create without replacement; a crash after creation consumes the retry."""
    w.require(not os.path.lexists(path), 'exact retry already reserved; inspect archived intent')
    fd, temporary = tempfile.mkstemp(prefix=path.name+'.', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.link(temporary, path)  # Atomic no-replace publication under the journal lock.
        os.unlink(temporary)
        try:
            directory = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
        except OSError as error:
            raise w.PostcommitError('retry archive created; durability uncertain; inspect before any action') from error
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def mempool(transport, bundle_id):
    value = transport.call('get_mempool_item_by_tx_id', {'tx_id':'0x'+bundle_id})
    w.require(type(value) is dict, 'mempool envelope')
    for key in ('transaction_id', 'tx_id', 'spend_bundle_name'):
        if value.get(key) is not None:
            w.require(live.normalized(value[key]) == bundle_id, 'mempool response identity')
    item = value.get('mempool_item')
    if value.get('success') is True and value.get('error') is None:
        w.require(type(item) is dict and live.normalized(item.get('spend_bundle_name')) == bundle_id,
                  'exact present mempool identity')
        for key in ('transaction_id', 'tx_id'):
            if item.get(key) is not None:
                w.require(live.normalized(item[key]) == bundle_id, 'mempool item identity')
        w.require(value.get('status') in (None, 'SUCCESS'), 'consistent present mempool status')
        return 'present'
    structured = value.get('structuredError')
    if not (value.get('success') is False and type(structured) is dict
            and structured.get('code') == 'TX_NOT_IN_MEMPOOL'):
        raise live.Inconclusive('mempool availability unknown; no retry authorized')
    data = structured.get('data')
    w.require(type(data) is dict and live.normalized(data.get('tx_id')) == bundle_id,
              'exact missing mempool identity')
    w.require(item is None and value.get('status') is None and value.get('traceback') is None
              and value.get('error') in (None, 'Tx 0x'+bundle_id+' not in mempool')
              and structured.get('message') in (None, 'Tx not in mempool'),
              'consistent missing mempool envelope')
    return 'absent; not proof of previous submission failure'


def retry(journal, transport, expected_bundle_id, executable, explicitly_retry=False):
    w.require(explicitly_retry is True, 'explicit exact-bundle retry flag required')
    w.hex32(expected_bundle_id)
    w.require(executable is not None, 'explicit locally built preflight executable required')
    live.begin(transport)
    with w.locked(journal):
        state = w.load(journal)  # Complete retained source admission, never journal authority alone.
        original = live.read_intent(journal)
        w.require(original['prepared']['spend_bundle_id'] == expected_bundle_id, 'reviewed bundle ID differs')
        path = archive_path(journal, expected_bundle_id)
        w.require(not os.path.lexists(path), 'exact retry already reserved; inspect archived intent')
        original = live.reconcile_locked(journal, transport, original)
        if original['status'] == 'confirmed':
            return dict(action='confirmed_without_push', intent=original)
        w.require(original['status'] in ('push_started', 'submitted', 'ambiguous'), 'only a previously attempted intent can be explicitly retried')
        w.require(state == original['original_state'] and w.token(state) == original['revision_token'],
                  'journal changed from original attempted step')
        context = mempool(transport, expected_bundle_id)
        if context == 'present':
            return dict(action='monitor_existing_mempool_bundle', bundle_id=expected_bundle_id,
                        http_receipts=live.receipts(transport))
        live.network(transport)
        peak = live.tip(transport)
        coin = live.fetch_coin(transport, original['prepared']['current_coin'], peak)
        if coin is None:
            raise live.Inconclusive('exact funding coin unavailable; no retry authorized')
        w.require(coin['spent'] == 0 and peak['height'] < 0xffffffff, 'confirmed unspent input and supported protocol required')
        live.stable_tip(transport, peak)
        fresh = prepare_step.prepare(journal, executable, peak['previous_transaction_height'])
        w.require(fresh['spend_bundle_hex'] == original['prepared']['spend_bundle_hex']
                  and fresh['spend_bundle_id'] == expected_bundle_id
                  and fresh['children'] == original['prepared']['children'] and fresh['fees'] == 0,
                  'retry must preserve exact bundle bytes, outputs and zero fee')
        live.stable_tip(transport, peak)
        w.require(w.token(w.load(journal)) == original['revision_token'], 'journal changed before retry reservation')
        raw = w.payload(live.intent_path(journal), live.MAX_BYTES)
        w.require(live.decode(raw) == original, 'intent changed before retry archive')
        archive_once(path, raw)
        context_path = path.with_name(path.name+'.context.json')
        archive_once(context_path, w.encode(dict(
            schema=1, scope='explicit exact-bundle retry context; absence does not prove failure',
            bundle_id=expected_bundle_id, revision_token=original['revision_token'],
            mempool=context, tip=peak, http_receipts=live.receipts(transport))))
        original['status'] = 'prepared'
        original['prepared'].update(measurement=fresh['costs'], measurement_protocol=live.PROTOCOL,
                                    host_executable_sha256=fresh['host_executable_sha256'])
        original['http_receipts'] = live.receipts(transport)
        live.check_intent(original)
        live.write_intent(live.intent_path(journal), original)
        revision = original['revision_token']
    # The sealed submission operation obtains the lock again and independently
    # rechecks the revision, coin, protocol and exact bundle before its POST.
    # A cooperating writer winning this gap consumes that same prepared attempt;
    # this call then fails closed rather than sending a third submission.
    return dict(action='one_explicit_exact_retry', original_intent_archive=str(path),
                recovery_context_archive=str(context_path),
                intent=live.submit(journal, transport, revision, explicitly_submit=True, executable=executable))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--journal', type=Path, required=True)
    parser.add_argument('--executable', type=Path, required=True)
    parser.add_argument('--expected-bundle-id', required=True)
    parser.add_argument('--retry-exact-bundle', action='store_true')
    args = parser.parse_args()
    import json
    print(json.dumps(retry(args.journal, live.CurlTransport(args.journal.with_name(args.journal.name+'.responses')), args.expected_bundle_id,
                           args.executable, args.retry_exact_bundle), sort_keys=True))


if __name__ == '__main__':
    main()
