use super::*;
use chia_bls::G2Element;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
};

fn wire(value: Value) -> Vec<u8> {
    let body = serde_json::to_vec(&value).unwrap();
    [
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes(),
        body,
    ]
    .concat()
}
fn serve(responses: Vec<Vec<u8>>) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}/", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        responses
            .into_iter()
            .map(|response| {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_request(&mut stream);
                let _ = stream.write_all(&response);
                request
            })
            .collect()
    });
    (origin, handle)
}
fn read_request(stream: &mut TcpStream) -> (String, Value) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
        assert!(bytes.len() <= MAX_PUSH_BYTES + 8192);
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..end]).unwrap();
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .unwrap()
                .parse()
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return (
                    headers.lines().next().unwrap().to_owned(),
                    serde_json::from_slice(&bytes[end + 4..]).unwrap(),
                );
            }
        }
    }
}
fn local(origin: &str) -> Testnet11Client {
    Testnet11Client::build(
        origin,
        false,
        Duration::from_secs(1),
        Duration::from_secs(2),
    )
    .unwrap()
}
fn network() -> Value {
    json!({"success":true,"network_name":"testnet11","network_prefix":"txch"})
}
fn record() -> Value {
    json!({"coin":{"parent_coin_info":prefixed([1;32]),"puzzle_hash":prefixed([2;32]),"amount":123},"confirmed_block_index":5,"spent_block_index":0,"spent":false,"coinbase":false,"timestamp":42})
}
fn bundle() -> SpendBundle {
    SpendBundle::new(
        vec![CoinSpend::new(
            Coin::new([1; 32].into(), [2; 32].into(), 123),
            Program::from(vec![1]),
            Program::from(vec![0x80]),
        )],
        G2Element::default(),
    )
}
#[test]
fn network_assertion_is_actual_labels_and_local_constants_not_invented_genesis_field() {
    let (origin, thread) = serve(vec![wire(network())]);
    let mut client = local(&origin);
    let identity = client.verify_network().unwrap();
    assert_eq!(
        identity.local_genesis_challenge,
        chia_sdk_types::TESTNET11_CONSTANTS
            .genesis_challenge
            .to_bytes()
    );
    assert_eq!(
        identity.local_agg_sig_me_domain,
        chia_sdk_types::TESTNET11_CONSTANTS
            .agg_sig_me_additional_data
            .to_bytes()
    );
    assert!(identity.scope.contains("no authenticated genesis"));
    assert_eq!(identity.origin, ORIGIN);
    assert_eq!(
        thread.join().unwrap()[0].0,
        "POST /get_network_info HTTP/1.1"
    );
    for value in [
        json!({"success":true,"network_name":"mainnet","network_prefix":"xch"}),
        json!({"success":true,"network_name":"testnet11","network_prefix":"xch"}),
    ] {
        let (origin, thread) = serve(vec![wire(value)]);
        let mut client = local(&origin);
        assert_eq!(
            client.verify_network().unwrap_err().kind,
            ErrorKind::NetworkMismatch
        );
        assert_eq!(
            client.push(&bundle()).unwrap_err().kind,
            ErrorKind::NetworkMismatch
        );
        assert_eq!(thread.join().unwrap().len(), 1);
    }
}
#[test]
fn strict_duplicate_json_and_integer_boolean_controls() {
    for body in [
        br#"{"success":true,"succe\u0073s":false}"#.as_slice(),
        br#"{"success":1}"#.as_slice(),
    ] {
        let response = [
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes(),
            body.to_vec(),
        ]
        .concat();
        let (origin, thread) = serve(vec![response]);
        assert_eq!(
            local(&origin).verify_network().unwrap_err().kind,
            ErrorKind::Decode
        );
        thread.join().unwrap();
    }
    for field in ["confirmed_block_index", "spent_block_index", "timestamp"] {
        let mut row = record();
        row[field] = json!(false);
        assert_eq!(record_from_value(&row).unwrap_err().kind, ErrorKind::Decode);
    }
    for value in [json!(true), json!(1.0), json!(-1), json!("1")] {
        assert!(uint64(&value).is_err());
    }
    let mut row = record();
    row["coin"]["amount"] = json!(true);
    assert!(record_from_value(&row).is_err());
    let mut row = record();
    row["extra"] = json!(0);
    assert!(record_from_value(&row).is_err());
}
#[test]
fn exact_point_lookup_distinguishes_null_missing_wrong_coin_and_provider_failure() {
    let good = record();
    let id = record_from_value(&good).unwrap().id();
    let (origin, thread) = serve(vec![
        wire(json!({"success":true,"coin_record":good})),
        wire(json!({"success":true,"coin_record":null})),
        wire(json!({"success":true})),
        wire(json!({"success":false,"error":"not found"})),
    ]);
    let mut client = local(&origin);
    assert_eq!(client.coin(id).unwrap().unwrap().id(), id);
    assert!(client.coin(id).unwrap().is_none());
    assert_eq!(client.coin(id).unwrap_err().kind, ErrorKind::Decode);
    assert_eq!(client.coin(id).unwrap_err().kind, ErrorKind::ProviderRpc);
    thread.join().unwrap();
    let (origin, thread) = serve(vec![wire(json!({"success":true,"coin_record":record()}))]);
    assert_eq!(
        local(&origin).coin([9; 32]).unwrap_err().kind,
        ErrorKind::InvalidRecord
    );
    thread.join().unwrap();
}
#[test]
fn candidate_set_without_completion_is_explicitly_partial_and_child_scan_inconclusive() {
    let row = record();
    let (origin, thread) = serve(vec![
        wire(json!({"success":true,"coin_records":[row.clone()]})),
        wire(json!({"success":true,"coin_records":[row]})),
    ]);
    let mut client = local(&origin);
    let candidates = client.address_coins([2; 32]).unwrap();
    assert!(!candidates.complete);
    assert_eq!(candidates.records.len(), 1);
    assert_eq!(
        client.children([1; 32], 5).unwrap_err().kind,
        ErrorKind::Inconclusive
    );
    thread.join().unwrap();
}
#[test]
fn complete_children_pagination_checks_cursor_parent_height_and_aggregate() {
    let mut second = record();
    second["coin"]["amount"] = json!(124);
    let (origin, thread) = serve(vec![
        wire(
            json!({"success":true,"coin_records":[record()],"truncated":true,"next_cursor":"next"}),
        ),
        wire(json!({"success":true,"coin_records":[second],"truncated":false,"next_cursor":null})),
    ]);
    let mut client = local(&origin);
    assert_eq!(client.children([1; 32], 5).unwrap().len(), 2);
    let requests = thread.join().unwrap();
    assert_eq!(requests[1].1["cursor"], "next");
    assert_eq!(requests[0].1["include_spent_coins"], true);
    for bad in [
        json!({"success":true,"coin_records":[record()],"truncated":false,"next_cursor":"unexpected"}),
        json!({"success":true,"coin_records":[record()],"truncated":true,"next_cursor":null}),
    ] {
        let (origin, thread) = serve(vec![wire(bad)]);
        assert_eq!(
            local(&origin).children([1; 32], 5).unwrap_err().kind,
            ErrorKind::Inconclusive
        );
        thread.join().unwrap();
    }
}
#[test]
fn synced_tip_requires_real_booleans_and_exact_hash_height() {
    for state in [
        json!({"sync":{"synced":true,"sync_mode":false},"peak":{"height":5,"header_hash":prefixed([3;32])}}),
        json!({"sync":{"synced":1,"sync_mode":false},"peak":{"height":5,"header_hash":prefixed([3;32])}}),
    ] {
        let good = state["sync"]["synced"].as_bool() == Some(true);
        let (origin, thread) = serve(vec![wire(json!({"success":true,"blockchain_state":state}))]);
        let result = local(&origin).synced_tip();
        assert_eq!(result.is_ok(), good);
        thread.join().unwrap();
    }
}
#[test]
fn exact_observed_spend_binds_coin_and_preserves_program_bytes() {
    let spend = bundle().coin_spends[0].clone();
    let id = spend.coin.coin_id().to_bytes();
    let solution = json!({"coin":{"parent_coin_info":prefixed([1;32]),"puzzle_hash":prefixed([2;32]),"amount":123},"puzzle_reveal":"0x01","solution":"0x80"});
    let (origin, thread) = serve(vec![wire(json!({"success":true,"coin_solution":solution}))]);
    let observed = local(&origin).observed_spend(id, 7).unwrap();
    assert_eq!(observed.coin_spend(), spend);
    let request = &thread.join().unwrap()[0].1;
    assert_eq!(request["coin_id"], prefixed(id));
    assert_eq!(request["height"], 7);
    for s in ["0x", "0x0", "0xzz"] {
        assert!(program_bytes(s).is_err());
    }
}
#[test]
fn push_is_explicit_exact_bundle_and_distinguishes_pending_reported_rejection() {
    let bundle = bundle();
    let id = bundle.name().to_bytes();
    let (origin, thread) = serve(vec![
        wire(network()),
        wire(json!({"success":true,"status":"SUCCESS"})),
        wire(json!({"success":false,"status":"PENDING","error":"MEMPOOL_CONFLICT"})),
        wire(json!({"success":false,"status":"FAILED","error":"BAD_AGGREGATE_SIGNATURE"})),
        wire(json!({"success":false,"error":"something unexpected"})),
    ]);
    let mut client = local(&origin);
    assert_eq!(
        client.push(&bundle).unwrap_err().kind,
        ErrorKind::NetworkMismatch
    );
    client.verify_network().unwrap();
    assert_eq!(
        client.push(&bundle).unwrap(),
        SubmissionOutcome::MempoolAccepted { transaction_id: id }
    );
    assert_eq!(
        client.push(&bundle).unwrap(),
        SubmissionOutcome::Pending { transaction_id: id }
    );
    assert_eq!(
        client.push(&bundle).unwrap(),
        SubmissionOutcome::Rejected {
            transaction_id: id,
            provider_code: "BAD_AGGREGATE_SIGNATURE".into()
        }
    );
    assert_eq!(
        client.push(&bundle).unwrap_err().kind,
        ErrorKind::Inconclusive
    );
    let requests = thread.join().unwrap();
    assert_eq!(requests.len(), 5);
    assert_eq!(
        requests[1].1["spend_bundle"]["coin_spends"][0]["puzzle_reveal"],
        "0x01"
    );
    assert_eq!(
        requests[1].1["spend_bundle"]["coin_spends"][0]["solution"],
        "0x80"
    );
    assert_eq!(
        requests[1].1["spend_bundle"]["aggregated_signature"],
        format!("0x{}", hex::encode(bundle.aggregated_signature.to_bytes()))
    );
    let mut duplicate = bundle.clone();
    duplicate.coin_spends.push(bundle.coin_spends[0].clone());
    assert!(bundle_request(&duplicate).is_err());
    let mut huge = bundle;
    huge.coin_spends[0].solution = Program::from(vec![0; MAX_PUSH_BYTES]);
    assert!(bundle_request(&huge).is_err());
}
#[test]
fn mempool_missing_requires_exact_structured_code_and_transaction_identity() {
    let id = [4; 32];
    let (origin, thread) = serve(vec![
        wire(
            json!({"success":false,"structuredError":{"code":"TX_NOT_IN_MEMPOOL","data":{"tx_id":prefixed(id)}}}),
        ),
        wire(
            json!({"success":true,"mempool_item":{"spend_bundle_name":prefixed(id),"cost":17,"fee":2}}),
        ),
        wire(json!({"success":false,"error":"not found"})),
    ]);
    let mut client = local(&origin);
    assert_eq!(
        client.mempool(id).unwrap(),
        MempoolOutcome::Missing { transaction_id: id }
    );
    assert_eq!(
        client.mempool(id).unwrap(),
        MempoolOutcome::Present {
            transaction_id: id,
            cost: 17,
            fee: 2
        }
    );
    assert_eq!(client.mempool(id).unwrap_err().kind, ErrorKind::ProviderRpc);
    thread.join().unwrap();
}
#[test]
fn pending_success_cannot_carry_errors_and_failed_codes_cannot_conflict() {
    let bundle = bundle();
    let cases = [
        json!({"success":true,"status":"PENDING","error":"MEMPOOL_CONFLICT"}),
        json!({"success":true,"status":"PENDING","structuredError":{"code":"MEMPOOL_CONFLICT"}}),
        json!({"success":false,"status":"PENDING","error":"MEMPOOL_CONFLICT","structuredError":{"code":"BAD_SIGNATURE"}}),
        json!({"success":false,"status":"FAILED","error":"BAD_SIGNATURE","structuredError":{"code":"DOUBLE_SPEND"}}),
        json!({"success":false,"status":"PENDING","error":true}),
    ];
    let mut responses = vec![wire(network())];
    responses.extend(cases.into_iter().map(wire));
    let (origin, thread) = serve(responses);
    let mut client = local(&origin);
    client.verify_network().unwrap();
    for _ in 0..5 {
        assert!(client.push(&bundle).is_err());
    }
    assert_eq!(thread.join().unwrap().len(), 6);
    let (origin, thread) = serve(vec![
        wire(network()),
        wire(
            json!({"success":false,"status":"FAILED","error":"DOUBLE_SPEND","structuredError":{"code":"DOUBLE_SPEND"}}),
        ),
    ]);
    let mut client = local(&origin);
    client.verify_network().unwrap();
    assert_eq!(
        client.push(&bundle).unwrap(),
        SubmissionOutcome::Rejected {
            transaction_id: bundle.name().to_bytes(),
            provider_code: "DOUBLE_SPEND".into()
        }
    );
    thread.join().unwrap();
}
#[test]
fn all_simultaneously_reported_transaction_ids_must_match() {
    let bundle = bundle();
    let id = bundle.name().to_bytes();
    let (origin, thread) = serve(vec![
        wire(network()),
        wire(
            json!({"success":true,"status":"SUCCESS","transaction_id":prefixed(id),"tx_id":prefixed([9;32])}),
        ),
    ]);
    let mut client = local(&origin);
    client.verify_network().unwrap();
    assert_eq!(
        client.push(&bundle).unwrap_err().kind,
        ErrorKind::InvalidRecord
    );
    thread.join().unwrap();
    let (origin, thread) = serve(vec![wire(
        json!({"success":true,"mempool_item":{"spend_bundle_name":prefixed(id),"tx_id":prefixed([9;32]),"cost":17,"fee":2}}),
    )]);
    assert_eq!(
        local(&origin).mempool(id).unwrap_err().kind,
        ErrorKind::InvalidRecord
    );
    thread.join().unwrap();
}
#[test]
fn mempool_missing_matches_actual_generic_message_and_rejects_contradictions() {
    let id = [4; 32];
    let good = json!({"success":false,"error":format!("Tx {} not in mempool",prefixed(id)),"structuredError":{"code":"TX_NOT_IN_MEMPOOL","data":{"tx_id":prefixed(id)},"message":"Tx not in mempool"},"traceback":null});
    let (origin, thread) = serve(vec![wire(good.clone())]);
    assert_eq!(
        local(&origin).mempool(id).unwrap(),
        MempoolOutcome::Missing { transaction_id: id }
    );
    thread.join().unwrap();
    let mut cases = Vec::new();
    for (field, value) in [
        ("error", json!("BAD_AGGREGATE_SIGNATURE")),
        ("status", json!("SUCCESS")),
        ("mempool_item", json!({"cost":1})),
        ("tx_id", json!(prefixed([9; 32]))),
        ("traceback", json!("provider traceback")),
    ] {
        let mut changed = good.clone();
        changed[field] = value;
        cases.push(changed);
    }
    let mut changed = good;
    changed["structuredError"]["message"] = json!("different error");
    cases.push(changed);
    for case in cases {
        let (origin, thread) = serve(vec![wire(case)]);
        assert_eq!(
            local(&origin).mempool(id).unwrap_err().kind,
            ErrorKind::InvalidRecord
        );
        thread.join().unwrap();
    }
    let (origin, thread) = serve(vec![wire(
        json!({"success":true,"status":"FAILED","mempool_item":{"spend_bundle_name":prefixed(id),"cost":17,"fee":2}}),
    )]);
    assert_eq!(
        local(&origin).mempool(id).unwrap_err().kind,
        ErrorKind::InvalidRecord
    );
    thread.join().unwrap();
}
#[test]
fn spent_height_zero_is_an_unspent_sentinel_not_a_live_spend() {
    let mut row = record();
    row["confirmed_block_index"] = json!(0);
    assert!(record_from_value(&row).is_ok());
    row["spent"] = json!(true);
    assert_eq!(
        record_from_value(&row).unwrap_err().kind,
        ErrorKind::InvalidRecord
    );
    row["spent_block_index"] = json!(1);
    assert!(record_from_value(&row).is_ok());
    row["confirmed_block_index"] = json!(2);
    assert_eq!(
        record_from_value(&row).unwrap_err().kind,
        ErrorKind::InvalidRecord
    );
}
#[test]
fn http_limits_status_redirect_and_streamed_body_are_bounded_no_retries() {
    for status in [302, 429, 503] {
        let (origin,thread)=serve(vec![format!("HTTP/1.1 {status} Status\r\nContent-Length: 2\r\nLocation: http://127.0.0.1:1/never\r\nConnection: close\r\n\r\n{{}}").into_bytes()]);
        assert_eq!(
            local(&origin).verify_network().unwrap_err().kind,
            if status == 429 {
                ErrorKind::RateLimited
            } else {
                ErrorKind::Http
            }
        );
        assert_eq!(thread.join().unwrap().len(), 1);
    }
    let (origin, thread) = serve(vec![
        b"HTTP/1.1 200 OK\r\nContent-Length: 65537\r\nConnection: close\r\n\r\n".to_vec(),
    ]);
    assert_eq!(
        local(&origin).verify_network().unwrap_err().kind,
        ErrorKind::ResponseLimit
    );
    thread.join().unwrap();
    let mut response =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n10001\r\n"
            .to_vec();
    response.extend_from_slice(&vec![b'x'; 65537]);
    response.extend_from_slice(b"\r\n0\r\n\r\n");
    let (origin, thread) = serve(vec![response]);
    assert_eq!(
        local(&origin).verify_network().unwrap_err().kind,
        ErrorKind::ResponseLimit
    );
    thread.join().unwrap();
}
#[test]
fn response_errors_never_echo_untrusted_secret_shaped_body_and_nested_runtime_refused() {
    let (origin, thread) = serve(vec![wire(
        json!({"success":false,"error":"sensitive-untrusted-provider-text"}),
    )]);
    let message = local(&origin).verify_network().unwrap_err().to_string();
    assert!(!message.contains("sensitive"));
    thread.join().unwrap();
    let client = Testnet11Client::new().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        assert!(Testnet11Client::new().is_err());
        drop(client);
    });
}
#[test]
fn actual_http_total_deadline_expires_and_does_not_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}/", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        thread::sleep(Duration::from_millis(300));
        let _ = stream.write_all(&wire(network()));
    });
    let mut client = Testnet11Client::build(
        &origin,
        false,
        Duration::from_millis(100),
        Duration::from_millis(150),
    )
    .unwrap();
    assert_eq!(
        client.verify_network().unwrap_err().kind,
        ErrorKind::Timeout
    );
    handle.join().unwrap();
}
#[test]
fn actual_streamed_body_deadline_is_not_only_header_deadline() {
    for continuous in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            if continuous {
                for _ in 0..12 {
                    if stream.write_all(b"1\r\nx\r\n").is_err() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(30));
                }
            } else {
                thread::sleep(Duration::from_millis(300));
                let _ = stream.write_all(b"2\r\n{}\r\n0\r\n\r\n");
            }
        });
        let mut client = Testnet11Client::build(
            &origin,
            false,
            Duration::from_millis(100),
            Duration::from_millis(150),
        )
        .unwrap();
        assert_eq!(
            client.verify_network().unwrap_err().kind,
            ErrorKind::Timeout
        );
        handle.join().unwrap();
    }
}
#[test]
fn failed_reverification_revokes_submission_gate() {
    let (origin, thread) = serve(vec![
        wire(network()),
        wire(json!({"success":true,"network_name":"mainnet","network_prefix":"xch"})),
    ]);
    let mut client = local(&origin);
    client.verify_network().unwrap();
    assert!(client.verify_network().is_err());
    assert_eq!(
        client.push(&bundle()).unwrap_err().kind,
        ErrorKind::NetworkMismatch
    );
    assert_eq!(thread.join().unwrap().len(), 2);
}
