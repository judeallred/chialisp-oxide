"""Version2 offline history packet: untouched v1 core + opaque block observations.

The v1 source/packet/handoffs remain historical and unchanged. Outer transport
admission checks every member before selecting core data. Block generators,
signatures and proofs receive JSON/byte bounds only; they are never CLVM trees.
The fixed CREATE rule is source-reviewed conditional arithmetic over captured
outputs, not an executed consensus parser or a universal redesign lower bound.
"""
import collections
import json
import math
import os
from pathlib import Path
import re
import stat

from cargo_source_receipt import decode_json
import xchandles_history as core
import xchandles_history_blocks as blocks

PROFILE = 'xchandles-mainnet-premine-history/v2'
PROTOCOL = 'chialisp-oxide-history-packet/v2'
MANIFEST = core.MANIFEST
COST_LICENSE_SHA = '7db0cf1060026bf2a544ea29cf9a96fba7d3781f93a1d06d6fee18251f1c207a'
# Exact Chia_rs LICENSE at56f6b676..., supplied explicitly by Root; it applies
# only to cost sources and does not relicense the separate Prosperity sources.
COST_PINS = {
    'count.py': '28fa9a90aa206ffdc78dd3863d8e37927a897bdf0b34567ba5bca590c3680998',
    'receipt.json': 'ddeaa696bfccfa3cc757ff4c4db8e9dcc7a812485e9f8f902b326f4ab85c1043',
    'source-rule-audit.json': '4d95e9b61d15a1e6a5e8b46fbece9dc61d1128b36d56ee881cd2b87ead5b17cc',
    'inventory.json': '5e465497dd54bd63f516517a3ea9c6e20e29dbc721e72e1695e159a8fa645bd1',
    'sources/.cargo_vcs_info.json': '787494bb7b738d358e722aa8068eae5d63d479f9b946ff138e7c7773d64d7760',
    'sources/src/opcodes.rs': '7d7eec6712d68a73ae891bf553ba47e66ace9810b98db428e1ca9e1bed7c37bc',
    'sources/src/conditions.rs': '91f868e4b8c6abdb868a59eb59cc19c215a0c498b966ae1c0eb2b398b165f8b7',
    'sources/src/consensus_constants.rs': 'e627a2123ebf75a2055430746fd1d2bed891d6ac7c7c2c16ef7cd46097176a5e',
    'sources/src/spendbundle_validation.rs': '0c50aaae1160c0e128239a494ad5045d68ea3e9c829949a81b18e4852d1d014d',
    'sources/src/flags.rs': '732ceb87d6ded878147a87ab1b4e593ac7687eebe7717f541a341477b6d14d50',
}
COST_SCOPE = ('Under the source-reviewed chia-consensus0.36.1 fixed CREATE_COIN rule, '
    'preserving the11508 captured zero-mojo creations alone charges20714400000, '
    'exceeding the pinned11000000000 block ceiling. This is conditional finite '
    'unchanged-output arithmetic, not measured full-bundle fees/cost, an executed '
    'consensus-parser proof, historical node authentication, minimal batch count, '
    'current/future pricing guarantee or lower bound on every redesigned protocol.')
SCOPE = (core.SCOPE + ' Version2 adds258 whole-block RPC observations, not258 more '
    'component executions. Block generators/signatures/proofs remain opaque provider '
    'data; no ordinary-tree validation, execution or chain authentication is credited. '
    + COST_SCOPE)


def json_value(raw, manifest=False):
    maximum = core.MAX_MANIFEST if manifest else core.MAX_MEMBER
    core.require(type(raw) is bytes and len(raw) <= maximum, 'v2 JSON byte limit')
    value = decode_json(raw, max_depth=64, max_nodes=100000 if manifest else 200000)
    pending = [(value, '')]
    while pending:
        item, key = pending.pop()
        if type(item) is float:
            core.require(key == 'elapsed_seconds' and math.isfinite(item) and item >= 0,
                         'float outside v2 timing role')
        if type(item) is dict:
            pending.extend((v, k) for k, v in item.items())
        elif type(item) is list:
            pending.extend((v, '') for v in item)
    return value



def member_cap(name):
    if name in (MANIFEST, 'core/'+MANIFEST):
        return core.MAX_MANIFEST
    if re.fullmatch(r'blocks/http/[0-9]{3}-get_block_record_by_height\.raw\.json', name):
        return 65536
    return core.MAX_MEMBER


def role(name):
    core.relative(name)
    if name.startswith('core/'):
        return 'preserved_primary_v1'
    if name.startswith('blocks/'):
        return 'opaque_block_observation'
    core.require(name.startswith('cost/') and name.removeprefix('cost/') in {*COST_PINS, 'LICENSE'},
                 'closed v2 member role')
    return 'pinned_cost_source_observation'


class Packet:
    """Independent outer precharge; no payload opens until inventory is complete."""
    def __init__(self, root):
        self.root = Path(root)
        core.require(self.root.is_dir() and not self.root.is_symlink(), 'v2 packet root')
        for parent in [self.root, *self.root.parents]:
            core.require(not parent.is_symlink(), 'v2 root ancestor alias')
        raw = core.read_regular(self.root/MANIFEST, core.MAX_MANIFEST)
        self.manifest = json_value(raw, manifest=True)
        self.manifest_identity = core.ident(raw)
        core.closed(self.manifest, ('protocol', 'profile', 'members'), 'v2 manifest')
        core.require(self.manifest['protocol'] == PROTOCOL and self.manifest['profile'] == PROFILE,
                     'v2 named profile')
        members = self.manifest['members']
        core.require(type(members) is list and 0 < len(members) < core.MAX_FILES, 'v2 declared count')
        self.rows, total = {}, len(raw)
        for row in members:
            core.closed(row, ('path', 'role', 'bytes', 'sha256'), 'v2 member')
            name = core.relative(row['path'])
            core.require(name != MANIFEST and name not in self.rows and row['role'] == role(name), 'v2 unique closed member')
            cap = member_cap(name)
            total += core.integer(row['bytes'], cap)
            core.require(total <= core.MAX_TOTAL and type(row['sha256']) is str
                         and re.fullmatch('[0-9a-f]{64}', row['sha256']), 'v2 aggregate/hash precharge')
            self.rows[name] = row
        self.physical, seen = {}, set()
        pending, entries = [(self.root, 0)], 0
        while pending:
            directory, depth = pending.pop()
            core.require(depth <= 8, 'v2 directory depth')
            with os.scandir(directory) as stream:
                for entry in stream:
                    entries += 1
                    core.require(entries <= 8192, 'v2 streamed entry bound')
                    st = entry.stat(follow_symlinks=False)
                    core.require(not stat.S_ISLNK(st.st_mode), 'v2 symlink')
                    if stat.S_ISDIR(st.st_mode):
                        pending.append((Path(entry.path), depth+1))
                        continue
                    name = core.relative(Path(entry.path).relative_to(self.root).as_posix())
                    core.require(stat.S_ISREG(st.st_mode) and st.st_nlink == 1
                                 and (st.st_dev, st.st_ino) not in seen, 'v2 regular nonalias')
                    seen.add((st.st_dev, st.st_ino))
                    core.require(len(self.physical) < core.MAX_FILES, 'v2 physical count')
                    cap = member_cap(name)
                    core.require(st.st_size <= cap, 'v2 physical size')
                    self.physical[name] = (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns)
        core.require(set(self.physical) == set(self.rows) | {MANIFEST}, 'v2 missing/extraneous member')
        for name, row in self.rows.items():
            core.require(self.physical[name][2] == row['bytes'], 'v2 physical/declared size')
        core.require(sum(s[2] for s in self.physical.values()) <= core.MAX_TOTAL, 'v2 physical aggregate')
        core.require(core.read_regular(self.root/MANIFEST, core.MAX_MANIFEST, self.physical[MANIFEST]) == raw,
                     'stable outer manifest')

    def raw(self, name):
        core.relative(name)
        core.require(name in self.rows, 'v2 undeclared payload')
        raw = core.read_regular(self.root/name, member_cap(name), self.physical[name])
        core.require(core.ident(raw) == {k: self.rows[name][k] for k in ('bytes', 'sha256')}, 'v2 member byte identity')
        return raw

    def json(self, name):
        return json_value(self.raw(name), manifest=name == 'core/'+MANIFEST)


def source_cost_observations(packet, core_receipt):
    """Pin exact source and manual audit statements; do not execute their code."""
    core.require(COST_LICENSE_SHA is not None and core.sha(packet.raw('cost/LICENSE')) == COST_LICENSE_SHA,
                 'pinned Apache2 license')
    for name, expected in COST_PINS.items():
        core.require(core.sha(packet.raw('cost/'+name)) == expected, 'exact source/count observation pin')
    expected_names = {'cost/'+n for n in (*COST_PINS, 'LICENSE')}
    inventory = packet.json('cost/inventory.json')
    core.closed(inventory, ('status', 'files'), 'cost source inventory')
    core.require(type(inventory['files']) is list and len(inventory['files']) == 9, 'cost nine historical inventory rows')
    for row in inventory['files']:
        core.closed(row, ('path', 'bytes', 'sha256'), 'cost source row')
        name = core.relative(row['path'])
        core.require(name in COST_PINS and name != 'inventory.json', 'cost historical inventory closure')
        raw = packet.raw('cost/'+name)
        core.require(core.integer(row['bytes'], core.MAX_MEMBER) == len(raw)
                     and blocks.same(core.ident(raw), {k: row[k] for k in ('bytes', 'sha256')}), 'cost source byte relation')
    core.require(len({r['path'] for r in inventory['files']}) == 9, 'cost inventory uniqueness')
    receipt = packet.json('cost/receipt.json')
    core.require(receipt['attempt4_receipt_sha256'] == core.sha(packet.raw('core/attempt4/execution-receipt.json')),
                 'cost/core original receipt binding')
    crate = receipt['crate']
    core.require(crate['name'] == 'chia-consensus' and crate['version'] == '0.36.1'
                 and crate['archive_sha256'] == 'd942bcb20b243213becbd94630dd7122c18dea7450beab38cbb4a6fcac993cae'
                 and crate['git_metadata_is_not_source_deployment_authentication'] is True, 'pinned cost crate metadata')
    vcs = packet.json('cost/sources/.cargo_vcs_info.json')
    core.require(vcs == {'git': {'sha1': '56f6b676a96de8659fe05d7e4f705df553f37fa5'}, 'path_in_vcs': 'crates/chia-consensus'}, 'pinned VCS observation')
    text = packet.raw('cost/sources/src/opcodes.rs').decode('utf-8')
    rates = re.findall(r'^pub const CREATE_COIN_COST: Cost = ([0-9_]+);$', text, re.M)
    core.require(len(rates) == 1 and int(rates[0].replace('_', '')) == 1800000, 'fixed source CREATE rate')
    # The full sources and manual source-rule audit are hash-pinned. This small
    # extraction is a source constant check, not a parser semantics proof.
    rule = packet.json('cost/source-rule-audit.json')
    core.require(rule['status'] == 'manual_pinned_consensus_rule_audit_complete'
        and rule['historical_applicability']['observed_height_range'] == [9174976, 9175930], 'manual source-rule scope')
    core.require(len(core_receipt['steps']) == 129, 'cost input component count')
    return receipt, expected_names


def count_creations(core_packet, steps):
    """Count actual retained outputs independently of auditor count metadata."""
    rows, all_ids, histogram = [], set(), collections.Counter()
    total, zeros, ones = 0, 0, 0
    for index, step in enumerate(steps):
        name = f'attempt4/steps/{index:03}.stdout.json'
        raw = core_packet.raw(name)
        value = core_packet.json(name)
        output = core.decode(core.unhex(value['outcome']['output_hex']))
        conditions = [core.fields(c, 32) for c in core.fields(output)]
        creates = [c for c in conditions if c and c[0] == b'3']
        seen, zero, one = set(), 0, 0
        for c in creates:
            core.require(len(c) == 4 and type(c[1]) is bytes and len(c[1]) == 32
                         and c[2] in (b'', b'\1'), 'captured canonical creation profile')
            amount = 0 if c[2] == b'' else 1
            pair = (c[1], amount)
            core.require(pair not in seen, 'duplicate same-parent output')
            seen.add(pair)
            coin = dict(parent_coin_info='0x'+step['coin_id'], puzzle_hash='0x'+c[1].hex(), amount=amount)
            cid = core.coin_id(coin)
            core.require(cid not in all_ids, 'duplicate created coin ID')
            all_ids.add(cid)
            zero += amount == 0
            one += amount == 1
        core.require(zero == len(step['handles'])*3 and one == 1, 'actual per-component creation counts')
        total += len(creates)
        zeros += zero
        ones += one
        histogram[str(len(creates))] += 1
        rows.append(dict(index=index, names=len(step['handles']), create_coin_conditions=len(creates),
            zero_mojo=zero, one_mojo=one, condition_cost_lower_bound=len(creates)*1800000,
            stdout_bytes=len(raw), stdout_sha256=core.sha(raw)))
    counts = dict(components=129, registered_names=3836, CREATE_COIN_total=total,
        zero_mojo_CREATE_COIN=zeros, one_mojo_controller_CREATE_COIN=ones,
        unique_created_coin_ids=len(all_ids), per_component_create_count_histogram=dict(histogram))
    core.require((total, zeros, ones, len(all_ids)) == (11637, 11508, 129, 11637), 'finite creation totals')
    return rows, counts


def verify(root):
    packet = Packet(root)
    # Complete outer hashing and bounded duplicate-key/token/depth JSON parsing
    # precede ALL selected CLVM decoding, including for last/unselected members.
    for name in packet.rows:
        raw = packet.raw(name)
        if name.endswith('.json'):
            packet.json(name)
    primary = core.Packet(packet.root/'core')
    core_manifest = packet.json('core/'+MANIFEST)
    expected = {'core/'+MANIFEST}
    for row in core_manifest['members']:
        name = 'core/'+row['path']
        core.require(name in packet.rows and blocks.same(
            {k: packet.rows[name][k] for k in ('bytes', 'sha256')},
            {k: row[k] for k in ('bytes', 'sha256')}), 'outer/preserved core inventory binding')
        expected.add(name)
    receipt = primary.json('attempt4/execution-receipt.json')
    # Full-block/cost validation has no tree decoding. Generators remain opaque.
    block_result = blocks.verify(packet, receipt)
    cost_receipt, cost_names = source_cost_observations(packet, receipt)
    expected |= block_result['expected'] | cost_names
    core.require(set(packet.rows) == expected, 'complete v2 semantic inventory before core decode')
    core.preflight_packet_artifacts(primary)
    core_summary = core.verify(packet.root/'core')
    cost_rows, counts = count_creations(primary, receipt['steps'])
    core.require(blocks.same(cost_rows, cost_receipt['rows'])
                 and blocks.same(counts, cost_receipt['counts']), 'independent output count/audit relations')
    declared = cost_receipt['cost']
    for key, value in (('CREATE_COIN_unit_cost', 1800000), ('maximum_block_cost', 11000000000),
            ('all_observed_creation_condition_charge', 20946600000),
            ('zero_mojo_creation_condition_charge_only', 20714400000),
            ('zero_only_excess_over_block_limit', 9714400000),
            ('consolidated_one_controller_plus_same_zero_creates', 20716200000)):
        core.require(core.integer(declared[key]) == value, 'conditional source-cost arithmetic')
    observed = {row['height']: row for row in block_result['rows']}
    ui_rows, cumulative = [], 0
    for step, creates in zip(receipt['steps'], cost_rows):
        index = step['index']
        block = observed[step['record']['spent_block_index']]
        stem = f'core/attempt4/steps/{index:03}'
        cumulative += len(step['handles'])
        ui_rows.append(dict(index=index, coin=step['coin_id'], successor=step['next_coin_id'],
            height=block['height'], namesCount=len(step['handles']), cumulativeNames=cumulative,
            registryRevealTreeHash=core.FULL, registryStateTreeHash=core.STATE,
            allowedActionMerkleRoot=core.ROOT, primaryCost=step['primary_cost'],
            createZero=creates['zero_mojo'], createOne=creates['one_mojo'],
            fullBlockCost=block['fullBlockCost'], fullBlockFeesDecimal=block['fullBlockFeesDecimal'],
            timestamp=block['timestamp'], headerReported=block['headerReported'], headerRecomputed=False,
            downloads=dict(program='core/attempt4/programs/'+core.FULL+'.clvm.hex',
                solution=stem+'.solution.hex', solutionAssembly=stem+'.solution.asm',
                command=stem+'.command.json', stdout=stem+'.stdout.json', resultMeta=stem+'.result-meta.json',
                blockRecord=block['blockRecordDownload'], fullBlock=block['fullBlockDownload'],
                blockObservation=block['blockObservationDownload'])))
    return dict(protocol=PROFILE, status='retained_consistency_passed',
        primary=core_summary, primaryComponentObservations=129, newReaderExecutions=0,
        packet=dict(manifest=dict(path=MANIFEST, **packet.manifest_identity),
            members=packet.manifest['members'], maximumPhysicalFiles=core.MAX_FILES,
            maximumAggregateBytes=core.MAX_TOTAL, maximumMemberBytes=core.MAX_MEMBER,
            maximumManifestBytes=core.MAX_MANIFEST,
            aliasPolicy='Unique normalized relative descriptors; complete physical inventory rejects duplicate paths, symlinks, nonregular files, hardlink counts and duplicate device/inode aliases before payload reads.'),
        identities=dict(launcherCoinId=core.SEED, registryReveal=receipt['steps'][0]['program_identity'],
            registryStateTreeHash=core.STATE, allowedActionMerkleRoot=core.ROOT,
            actionLayerModuleTreeHash=core.ACTION,
            registerModule=dict(bytes=1478, blobSha256=core.REGISTER_BLOB, treeHash=core.REGISTER),
            sourceCorrespondence=core.provenance_record(),
            artifactCorrespondenceOnly=True, sourceDeploymentAuthenticated=False),
        blockRpcObservations=258, blockHeights=129, failedBlockCaptureAttemptsCredited=0,
        wholeBlockCostSum=block_result['costSum'], wholeBlockFeesSumDecimal=block_result['feesSumDecimal'],
        blockScope=blocks.SCOPE, creationCounts=counts,
        conditionalCreateCost=dict(unit=1800000, zeroOnly=20714400000, pinnedBlockCeiling=11000000000,
            scope=COST_SCOPE, sourceRuleDownload='cost/source-rule-audit.json',
            countObservationDownload='cost/receipt.json', parserSourceDownload='cost/sources/src/conditions.rs',
            constantSourceDownload='cost/sources/src/opcodes.rs', licenseDownload='cost/LICENSE'),
        rows=ui_rows, physicalFiles=len(packet.physical), physicalBytes=sum(v[2] for v in packet.physical.values()),
        manifestBytes=packet.physical[MANIFEST][2], scope=SCOPE)



def explicit_path(path, *, exists, directory):
    """Canonical explicit path, rejecting aliases before normalization/writes.

    Reject '..' rather than giving it surprising semantics across a symlink or
    nonexistent component. lstat sees dangling symlinks, unlike exists().
    """
    path = Path(path)
    core.require('..' not in path.parts, 'explicit path parent traversal')
    absolute = path if path.is_absolute() else Path.cwd()/path
    current = Path(absolute.anchor)
    for index, part in enumerate(absolute.parts[1:]):
        current = current/part
        try:
            st = current.lstat()
        except FileNotFoundError:
            core.require(not exists, 'required explicit input absent')
            continue
        core.require(not stat.S_ISLNK(st.st_mode), 'explicit path symlink ancestor')
        last = index == len(absolute.parts)-2
        if not last or directory:
            core.require(stat.S_ISDIR(st.st_mode), 'explicit path directory component')
    canonical = absolute.resolve(strict=exists)
    if exists:
        core.require(canonical.is_dir() if directory else canonical.is_file(), 'explicit input kind')
    else:
        # lstat above rejects even dangling symlinks at the intended leaf.
        core.require(not canonical.exists(), 'new v2 destination')
    return canonical


def guarded_inputs(primary_path, block_path, cost_path, license_path, destination):
    inputs = [explicit_path(p, exists=True, directory=True)
              for p in (primary_path, block_path, cost_path)]
    license_path = explicit_path(license_path, exists=True, directory=False)
    destination = explicit_path(destination, exists=False, directory=True)
    for source in inputs:
        core.require(not destination.is_relative_to(source),
                     'v2 destination must not mutate original input trees')
    # Case-insensitive filesystems may retain alternate spelling after resolve.
    # Compare existing ancestor identities as well as canonical relative paths.
    source_ids = {(p.stat().st_dev, p.stat().st_ino) for p in inputs}
    for parent in destination.parents:
        try:
            st = parent.lstat()
        except FileNotFoundError:
            continue
        core.require(not stat.S_ISLNK(st.st_mode)
                     and (st.st_dev, st.st_ino) not in source_ids,
                     'v2 destination aliases an original input tree')
    return (*inputs, license_path, destination)


def scan_inputs(base):
    """Bounded metadata-only input discovery; no aliases or hidden origins."""
    base = Path(base)
    for parent in [base, *base.parents]:
        core.require(not parent.is_symlink(), 'wrapper root ancestor alias')
    pending, found, entries = [(base, 0)], {}, 0
    while pending:
        directory, depth = pending.pop()
        core.require(depth <= 8, 'wrapper directory depth')
        with os.scandir(directory) as stream:
            for entry in stream:
                entries += 1
                core.require(entries <= 8192, 'wrapper directory entries')
                st = entry.stat(follow_symlinks=False)
                core.require(not stat.S_ISLNK(st.st_mode), 'wrapper input alias')
                path = Path(entry.path)
                if stat.S_ISDIR(st.st_mode):
                    pending.append((path, depth+1))
                else:
                    core.require(stat.S_ISREG(st.st_mode) and st.st_nlink == 1
                        and st.st_size <= core.MAX_MEMBER and len(found) < core.MAX_FILES, 'wrapper input member')
                    found[path.relative_to(base).as_posix()] = (path, st.st_size)
    return found


def build_packet(primary_path, block_path, cost_path, license_path, destination):
    """New offline wrapper; all source paths are explicit caller arguments."""
    primary_path, block_path, cost_path, license_path, destination = guarded_inputs(
        primary_path, block_path, cost_path, license_path, destination)
    source_sets = [('core', scan_inputs(primary_path)), ('blocks', scan_inputs(block_path)),
                   ('cost', scan_inputs(cost_path))]
    core.require(len(source_sets[0][1]) == 1057 and len(source_sets[1][1]) == 907
                 and set(source_sets[2][1]) == set(COST_PINS), 'closed successful input sets')
    sources = {prefix+'/'+name: item for prefix, files in source_sets for name, item in files.items()}
    license_path = Path(license_path)
    st = license_path.lstat()
    core.require(stat.S_ISREG(st.st_mode) and st.st_nlink == 1 and st.st_size <= core.MAX_MEMBER,
                 'explicit regular license input')
    sources['cost/LICENSE'] = (license_path, st.st_size)
    for name, (_, length) in sources.items():
        core.require(length <= member_cap(name), 'wrapper per-role precharge')
    core.require(len(sources)+1 <= core.MAX_FILES
                 and sum(n for _, n in sources.values())+core.MAX_MANIFEST <= core.MAX_TOTAL,
                 'complete v2 wrapper precharge')
    # All physical lengths/counts are admitted before copying any payload.
    destination.mkdir(parents=True)
    rows = []
    for name, (path, length) in sorted(sources.items()):
        core.relative(name)
        raw = core.read_regular(path, member_cap(name))
        core.require(len(raw) == length, 'stable wrapper declared length')
        out = destination/name
        out.parent.mkdir(parents=True, exist_ok=True)
        with out.open('xb') as stream:
            stream.write(raw)
        rows.append(dict(path=name, role=role(name), **core.ident(raw)))
    manifest = dict(protocol=PROTOCOL, profile=PROFILE, members=rows)
    raw = (json.dumps(manifest, indent=2)+'\n').encode()
    core.require(len(raw) <= core.MAX_MANIFEST, 'unchanged512KiB manifest bound')
    with (destination/MANIFEST).open('xb') as stream:
        stream.write(raw)
    return verify(destination)
