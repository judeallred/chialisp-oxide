import hashlib
import json
from pathlib import Path
import sys
import time
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'experiments/authenticated-continuations'))
import live_workflow as live
import workflow as w

BASE = Path(__file__).resolve().parent
JOURNAL = BASE/'journal.json'
EXE = ROOT/'target/continuation-prepare-private/debug/prepare-step'
RUNTIME = [ROOT/'experiments/authenticated-continuations'/name for name in
           ('live_workflow.py', 'workflow.py', 'prepare_step.py')]
snapshot = lambda: {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in RUNTIME}
frozen = snapshot()
deadline = time.monotonic()+1800

def event(kind, **data):
    value = dict(time=datetime.now(timezone.utc).isoformat(), kind=kind, **data)
    with (BASE/'advancement-events.jsonl').open('a') as stream:
        stream.write(json.dumps(value, sort_keys=True)+'\n')
    print(json.dumps(value), flush=True)

event('started', runtime_sha256=frozen, policy='no automatic retry; zero-fee fixed queue only')
while time.monotonic() < deadline:
    if snapshot() != frozen:
        raise RuntimeError('runtime source changed; inspect journal before proceeding')
    try:
        state = w.load(JOURNAL)
        index = state['completed']
        if index == 4:
            event('complete', completed=4, payout_mojos=4000)
            break
        intent = live.read_intent(JOURNAL)
        intent = live.reconcile(JOURNAL, live.CurlTransport())
        if intent['status'] == 'confirmed':
            old_index = intent['original_state']['completed']
            (BASE/f'step-{old_index}-confirmed.json').write_text(json.dumps(intent, indent=2)+'\n')
            state = w.load(JOURNAL)
            event('confirmed', index=old_index, spent_height=intent['confirmation']['spent_height'],
                  bundle_id=intent['prepared']['spend_bundle_id'])
            if state['completed'] == 4:
                continue
            transport = live.CurlTransport()
            live.network(transport)
            peak = live.tip(transport)
            intent = live.prepare(JOURNAL, EXE, peak['previous_transaction_height'], w.token(state))
            (BASE/f'step-{state["completed"]}-prepared.json').write_text(json.dumps(intent, indent=2)+'\n')
        if intent['status'] == 'prepared':
            state = w.load(JOURNAL)
            intent = live.submit(JOURNAL, live.CurlTransport(JOURNAL.with_name(JOURNAL.name+'.responses')),
                                 w.token(state), explicitly_submit=True, executable=EXE)
            (BASE/f'step-{state["completed"]}-submit.json').write_text(json.dumps(intent, indent=2)+'\n')
            event('submitted_once', index=state['completed'], status=intent['status'],
                  bundle_id=intent['prepared']['spend_bundle_id'])
        else:
            event('waiting', index=index, status=intent['status'])
    except live.Inconclusive as error:
        event('inconclusive_read_or_preflight', detail=str(error))
    time.sleep(20)
else:
    event('bounded_monitor_stopped', detail='1800-second monitoring limit; no push retry authorized')
