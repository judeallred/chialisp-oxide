use super::*;
use std::collections::VecDeque;

struct Mock {
    replies: VecDeque<(Endpoint, Value, std::result::Result<Value, ErrorKind>)>,
    calls: usize,
}
impl Mock {
    fn new(rows: Vec<(Endpoint, Value, Value)>) -> Self {
        Self {
            replies: rows
                .into_iter()
                .map(|(endpoint, request, response)| (endpoint, request, Ok(response)))
                .collect(),
            calls: 0,
        }
    }
    fn finished(&self) {
        assert!(self.replies.is_empty());
    }
}
impl Transport for Mock {
    fn post(&mut self, request: &Request) -> Result<Response> {
        self.calls += 1;
        let (endpoint, body, response) = self.replies.pop_front().expect("unexpected provider I/O");
        assert_eq!(request.endpoint(), endpoint);
        assert!(request.body().len() <= MAX_REQUEST);
        assert_eq!(
            serde_json::from_slice::<Value>(request.body()).unwrap(),
            body
        );
        let response =
            response.map_err(|kind| Error::new(kind, "constructed transport failure"))?;
        let bytes = serde_json::to_vec(&response).unwrap();
        let mut collector = BodyCollector::default();
        for chunk in bytes.chunks(17) {
            collector.push(chunk)?;
        }
        Ok(Response {
            status: 200,
            body: collector.finish()?,
        })
    }
}
fn input() -> CoinRecord {
    CoinRecord {
        parent: [0x11; 32],
        puzzle_hash: [0x22; 32],
        amount: 1000,
    }
}
fn child(tag: u8) -> CoinRecord {
    CoinRecord {
        parent: input().id(),
        puzzle_hash: [tag; 32],
        amount: 500,
    }
}
fn network() -> Value {
    json!({"success":true,"network_name":"testnet11","network_prefix":"txch","genesis_challenge":prefixed(&testnet11_network().genesis_challenge)})
}
fn tip(height: u32, tag: u8) -> Value {
    json!({"success":true,"blockchain_state":{"sync":{"synced":true,"sync_mode":false},"peak":{"height":height,"header_hash":prefixed(&[tag;32])}}})
}
fn record(coin: &CoinRecord, spent: bool) -> Value {
    json!({"coin":{"parent_coin_info":prefixed(&coin.parent),"puzzle_hash":prefixed(&coin.puzzle_hash),"amount":coin.amount},
        "confirmed_block_index":if coin.parent == input().id() {10} else {5},"spent":spent,
        "spent_block_index":if spent {12} else {0},"coinbase":false,"timestamp":123})
}
fn input_record(spent: bool) -> Value {
    let mut value = record(&input(), spent);
    if spent {
        value["spent_block_index"] = json!(10);
    }
    json!({"success":true,"coin_record":value})
}
fn query(cursor: Option<&str>) -> Value {
    let mut value = json!({"parent_ids":[prefixed(&input().id())],"include_spent_coins":true,"start_height":10,"end_height":11,"limit":257,"order":"asc"});
    if let Some(cursor) = cursor {
        value["cursor"] = json!(cursor);
    }
    value
}
fn prefix(coin: Value) -> Vec<(Endpoint, Value, Value)> {
    vec![
        (Endpoint::GetNetworkInfo, json!({}), network()),
        (Endpoint::GetBlockchainState, json!({}), tip(20, 1)),
        (
            Endpoint::GetCoinRecordByName,
            json!({"name":prefixed(&input().id())}),
            coin,
        ),
    ]
}
fn page(records: Vec<Value>, truncated: bool, cursor: Option<&str>) -> Value {
    let mut value = json!({"success":true,"coin_records":records,"truncated":truncated});
    if let Some(cursor) = cursor {
        value["next_cursor"] = json!(cursor);
    }
    value
}
fn snapshot(rows: Vec<(Endpoint, Value, Value)>) -> Snapshot {
    let mut mock = Mock::new(rows);
    let result = observe(&mut mock, &input()).unwrap();
    mock.finished();
    result
}
fn reject(rows: Vec<(Endpoint, Value, Value)>, kind: ErrorKind) {
    let mut mock = Mock::new(rows);
    assert_eq!(observe(&mut mock, &input()).err().unwrap().kind, kind);
    mock.finished();
}

#[test]
fn point_lookup_missing_unspent_and_provider_failure_stay_distinct() {
    for (value, expected) in [
        (
            json!({"success":true,"coin_record":null}),
            InputState::Missing,
        ),
        (
            input_record(false),
            InputState::Unspent { created_height: 5 },
        ),
    ] {
        let mut rows = prefix(value);
        rows.push((Endpoint::GetBlockchainState, json!({}), tip(20, 1)));
        let result = snapshot(rows);
        assert_eq!(result.observation.input, expected);
        assert_eq!(result.observation.input_id, input().id());
        assert!(result.observation.children.is_empty());
        assert_eq!(result.receipts.len(), 4);
    }
    reject(
        prefix(json!({"success":false,"error":"not found"})),
        ErrorKind::ProviderRpc,
    );
    reject(prefix(json!({"success":true})), ErrorKind::Decode);
    reject(
        prefix(json!({"success":true,"error":"failure","coin_record":null})),
        ErrorKind::ProviderRpc,
    );
}

#[test]
fn complete_pages_keep_spent_and_unexpected_children_and_exact_wire() {
    let mut rows = prefix(input_record(true));
    rows.push((
        Endpoint::GetCoinRecordsByParentIds,
        query(None),
        page(vec![record(&child(3), true)], true, Some("opaqueA")),
    ));
    rows.push((
        Endpoint::GetCoinRecordsByParentIds,
        query(Some("opaqueA")),
        page(vec![record(&child(4), false)], false, None),
    ));
    rows.push((Endpoint::GetBlockchainState, json!({}), tip(20, 1)));
    let result = snapshot(rows);
    assert_eq!(result.observation.children.len(), 2);
    assert!(
        result
            .observation
            .children
            .iter()
            .any(|row| row.coin == child(3))
    );
    assert!(
        result
            .observation
            .children
            .iter()
            .any(|row| row.coin == child(4))
    );
    assert!(matches!(
        result.observation.input,
        InputState::Spent {
            spent_height: 10,
            ..
        }
    ));
    assert_eq!(result.receipts.len(), 6);
    assert!(
        result
            .receipts
            .iter()
            .all(|row| row.response_sha256.len() == 64 && row.response_bytes <= MAX_BODY)
    );
}

#[test]
fn tip_change_lower_height_fork_and_unsynced_never_make_snapshot() {
    for after in [tip(21, 2), tip(19, 3), tip(20, 4)] {
        let mut rows = prefix(input_record(false));
        rows.push((Endpoint::GetBlockchainState, json!({}), after));
        reject(rows, ErrorKind::Incomplete);
    }
    let mut value = tip(20, 1);
    value["blockchain_state"]["sync"]["synced"] = json!(false);
    reject(
        vec![
            (Endpoint::GetNetworkInfo, json!({}), network()),
            (Endpoint::GetBlockchainState, json!({}), value),
        ],
        ErrorKind::Incomplete,
    );
}

#[test]
fn wrong_network_genesis_and_prefix_fail_before_coin_queries() {
    for (field, value) in [
        ("network_name", json!("mainnet")),
        ("network_prefix", json!("xch")),
        ("genesis_challenge", json!(prefixed(&[1; 32]))),
    ] {
        let mut response = network();
        response[field] = value;
        reject(
            vec![(Endpoint::GetNetworkInfo, json!({}), response)],
            ErrorKind::NetworkMismatch,
        );
    }
}

#[test]
fn parent_pages_require_explicit_complete_marker_and_valid_progress() {
    let invalid = [
        json!({"success":true,"coin_records":[]}),
        page(vec![], true, None),
        page(vec![], true, Some("")),
        page(vec![], true, Some(&"x".repeat(MAX_CURSOR + 1))),
        page(vec![], false, Some("spurious")),
    ];
    for response in invalid {
        let mut rows = prefix(input_record(true));
        rows.push((Endpoint::GetCoinRecordsByParentIds, query(None), response));
        reject(rows, ErrorKind::Incomplete);
    }
    let mut rows = prefix(input_record(true));
    rows.push((
        Endpoint::GetCoinRecordsByParentIds,
        query(None),
        page(vec![], true, Some("repeat")),
    ));
    rows.push((
        Endpoint::GetCoinRecordsByParentIds,
        query(Some("repeat")),
        page(vec![], true, Some("repeat")),
    ));
    reject(rows, ErrorKind::Incomplete);
}

#[test]
fn empty_progress_page_chain_has_finite_sixteen_page_budget() {
    let mut rows = prefix(input_record(true));
    for index in 0..MAX_PAGES {
        let previous = if index == 0 {
            None
        } else {
            Some(format!("page{}", index - 1))
        };
        rows.push((
            Endpoint::GetCoinRecordsByParentIds,
            query(previous.as_deref()),
            page(vec![], true, Some(&format!("page{index}"))),
        ));
    }
    let mut mock = Mock::new(rows);
    assert_eq!(
        observe(&mut mock, &input()).err().unwrap().kind,
        ErrorKind::Incomplete
    );
    assert_eq!(mock.calls, 3 + MAX_PAGES);
    mock.finished();
}

#[test]
fn duplicate_children_wrong_parent_height_and_inconsistent_state_fail() {
    let mut response = record(&child(4), false);
    response["coin"]["parent_coin_info"] = json!(prefixed(&[3; 32]));
    let mut height = record(&child(4), false);
    height["confirmed_block_index"] = json!(11);
    let mut state = record(&child(4), false);
    state["spent_block_index"] = json!(12);
    for value in [response, height, state] {
        let mut rows = prefix(input_record(true));
        rows.push((
            Endpoint::GetCoinRecordsByParentIds,
            query(None),
            page(vec![value], false, None),
        ));
        reject(rows, ErrorKind::InvalidRecord);
    }
    let mut rows = prefix(input_record(true));
    rows.push((
        Endpoint::GetCoinRecordsByParentIds,
        query(None),
        page(vec![record(&child(4), false)], true, Some("next")),
    ));
    rows.push((
        Endpoint::GetCoinRecordsByParentIds,
        query(Some("next")),
        page(vec![record(&child(4), false)], false, None),
    ));
    reject(rows, ErrorKind::Incomplete);
}

#[test]
fn wrong_input_or_future_spent_height_is_not_missing() {
    let mut wrong = input_record(false);
    wrong["coin_record"]["coin"]["amount"] = json!(999);
    reject(prefix(wrong), ErrorKind::InvalidRecord);
    let mut future = input_record(true);
    future["coin_record"]["spent_block_index"] = json!(21);
    reject(prefix(future), ErrorKind::InvalidRecord);
}

#[test]
fn aggregate_limit_is_across_pages_not_per_request() {
    // Split across pages so every response fits64KiB.257 independently valid
    // children must never be trimmed into a falsely complete256-child set.
    let records = (0..257u16)
        .map(|tag| {
            let mut coin = child(9);
            coin.amount = u64::from(tag);
            record(&coin, false)
        })
        .collect::<Vec<_>>();
    let mut rows = prefix(input_record(true));
    rows.push((
        Endpoint::GetCoinRecordsByParentIds,
        query(None),
        page(records[..128].to_vec(), true, Some("more")),
    ));
    rows.push((
        Endpoint::GetCoinRecordsByParentIds,
        query(Some("more")),
        page(records[128..].to_vec(), false, None),
    ));
    reject(rows, ErrorKind::Incomplete);
}

#[test]
fn chunk_limit_is_charged_before_copy_and_remains_poisoned() {
    let mut body = BodyCollector::default();
    body.push(&vec![0; MAX_BODY]).unwrap();
    assert!(body.push(&[1]).is_err());
    assert!(body.push(&[]).is_err());
    assert!(body.finish().is_err());
    let mut body = BodyCollector::default();
    assert!(body.push(&vec![0; MAX_BODY + 1]).is_err());
    assert!(body.finish().is_err());
    let mut body = BodyCollector::default();
    body.push(&vec![0; MAX_BODY]).unwrap();
    assert_eq!(body.finish().unwrap().as_bytes().len(), MAX_BODY);
}

struct Broken {
    status: u16,
    bytes: Vec<u8>,
    failure: Option<ErrorKind>,
}
impl Transport for Broken {
    fn post(&mut self, _: &Request) -> Result<Response> {
        if let Some(kind) = self.failure {
            return Err(Error::new(kind, "constructed"));
        }
        let mut body = BodyCollector::default();
        body.push(&self.bytes)?;
        Ok(Response {
            status: self.status,
            body: body.finish()?,
        })
    }
}
#[test]
fn http_timeout_transport_json_body_and_rpc_errors_are_not_consensus() {
    for (status,bytes,failure,expected) in [
        (429,b"{}".to_vec(),None,ErrorKind::RateLimited),
        (413,b"{}".to_vec(),None,ErrorKind::Http),
        (503,b"{}".to_vec(),None,ErrorKind::Http),
        (302,b"{}".to_vec(),None,ErrorKind::Http),
        (200,b"not json".to_vec(),None,ErrorKind::Decode),
        (200,vec![0;MAX_BODY+1],None,ErrorKind::BodyLimit),
        (200,b"{}".to_vec(),Some(ErrorKind::Timeout),ErrorKind::Timeout),
        (200,b"{}".to_vec(),Some(ErrorKind::Transport),ErrorKind::Transport),
        (200,br#"{"success":false,"structuredError":{"code":"BadAggregateSignature","message":"untrusted"}}"#.to_vec(),None,ErrorKind::ProviderRpc),
    ] {
        let mut transport=Broken{status,bytes,failure};
        assert_eq!(observe(&mut transport,&input()).err().unwrap().kind,expected);
    }
}

#[test]
fn request_serialization_and_session_io_allowances_are_bounded() {
    let mut writer = bounded::RequestWriter(vec![]);
    assert!(serde_json::to_writer(&mut writer, &"x".repeat(MAX_REQUEST + 1)).is_err());
    assert!(writer.0.len() <= MAX_REQUEST);
    let mut mock = Mock::new(vec![]);
    let mut session = Session {
        transport: &mut mock,
        remaining: 0,
        receipts: vec![],
    };
    assert_eq!(
        session
            .call(Endpoint::GetNetworkInfo, &json!({}))
            .err()
            .unwrap()
            .kind,
        ErrorKind::Incomplete
    );
    assert_eq!(mock.calls, 0);
}

#[test]
fn duplicate_keys_at_every_level_and_contradictory_errors_are_rejected() {
    // Exercise the actual parser boundary, rather than a Value fixture that
    // would already have discarded duplicate fields before transport.
    for bytes in [
        br#"{"success":false,"success":true}"#.as_slice(),
        br#"{"success":true,"truncated":true,"truncated":false}"#.as_slice(),
        br#"{"success":true,"coin_record":{"coin":{"amount":999,"amount":1000}}}"#.as_slice(),
        br#"{"success":true,"coin_record":{"coin":{"puzzle_hash":"11","puzzle_hash":"22"}}}"#
            .as_slice(),
        br#"{"success":true,"nested":[{"a":1,"a":2}]}"#.as_slice(),
    ] {
        let mut transport = Broken {
            status: 200,
            bytes: bytes.to_vec(),
            failure: None,
        };
        assert_eq!(
            observe(&mut transport, &input()).err().unwrap().kind,
            ErrorKind::Decode
        );
    }
    let mut transport = Broken {
        status: 200,
        bytes: br#"{"success":true,"structuredError":{"code":"contradiction"}}"#.to_vec(),
        failure: None,
    };
    assert_eq!(
        observe(&mut transport, &input()).err().unwrap().kind,
        ErrorKind::ProviderRpc
    );
    let mut transport = Broken {
        status: 200,
        bytes: format!("{}0{}", "[".repeat(130), "]".repeat(130)).into_bytes(),
        failure: None,
    };
    assert_eq!(
        observe(&mut transport, &input()).err().unwrap().kind,
        ErrorKind::Decode
    );
}
