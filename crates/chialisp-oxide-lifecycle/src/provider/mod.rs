//! Bounded, transport-neutral Testnet11 provider observations.
//! Fixed read-only HTTPS, no transaction push, signature service or chain proof.
use crate::{ChildObservation, CoinRecord, InputState, Network, Observation, Tip};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

mod bounded;
mod https;
mod json;
pub use bounded::{BodyCollector, BoundedBody, MAX_BODY, MAX_REQUEST};
pub use https::Testnet11Https;
#[cfg(test)]
mod tests;

pub const TESTNET11_ORIGIN: &str = "https://testnet11.api.coinset.org";
pub const MAX_CHILDREN: usize = 256;
pub const MAX_PAGES: usize = 16;
pub const MAX_CURSOR: usize = 256;
pub const MAX_CALLS: usize = MAX_PAGES + 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Transport,
    Timeout,
    BodyLimit,
    RequestLimit,
    Http,
    RateLimited,
    Decode,
    ProviderRpc,
    NetworkMismatch,
    InvalidRecord,
    Incomplete,
}
#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}
impl Error {
    pub fn new(kind: ErrorKind, message: &str) -> Self {
        Self {
            kind,
            message: message.chars().take(512).collect(),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

/// Closed read-only endpoint inventory: no caller-selected URLs or push method.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Endpoint {
    GetNetworkInfo,
    GetBlockchainState,
    GetCoinRecordByName,
    GetCoinRecordsByParentIds,
}
impl Endpoint {
    pub fn path(self) -> &'static str {
        match self {
            Self::GetNetworkInfo => "get_network_info",
            Self::GetBlockchainState => "get_blockchain_state",
            Self::GetCoinRecordByName => "get_coin_record_by_name",
            Self::GetCoinRecordsByParentIds => "get_coin_records_by_parent_ids",
        }
    }
}
pub struct Request {
    endpoint: Endpoint,
    body: Vec<u8>,
}
impl Request {
    pub fn endpoint(&self) -> Endpoint {
        self.endpoint
    }
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}
pub struct Response {
    pub status: u16,
    pub body: BoundedBody,
}
/// A future HTTPS backend must enforce its own finite wall-clock deadlines,
/// TLS and origin policy, no redirects, and feed chunks into BodyCollector.
/// This synchronous trait is for one serialized bounded observation; call it
/// outside an HTTP runtime worker. It is not a process/network sandbox.
pub trait Transport {
    fn post(&mut self, request: &Request) -> Result<Response>;
}

#[derive(Debug, Serialize)]
pub struct HttpReceipt {
    pub endpoint: Endpoint,
    pub status: u16,
    pub response_bytes: usize,
    pub response_sha256: String,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub observation: Observation,
    pub receipts: Vec<HttpReceipt>,
    pub scope: &'static str,
}

pub fn testnet11_network() -> Network {
    Network {
        testnet: "testnet11".into(),
        genesis_challenge: chia_sdk_types::TESTNET11_CONSTANTS
            .genesis_challenge
            .to_bytes(),
    }
}

/// A snapshot has at most 20 calls, 80 KiB request allowances and 1.25 MiB
/// response allowances. Each allowance is charged before serialization/I/O.
struct Session<'a, T> {
    transport: &'a mut T,
    remaining: usize,
    receipts: Vec<HttpReceipt>,
}
impl<T: Transport> Session<'_, T> {
    fn call(&mut self, endpoint: Endpoint, value: &impl Serialize) -> Result<Value> {
        if self.remaining == 0 {
            return Err(Error::new(ErrorKind::Incomplete, "snapshot request budget"));
        }
        self.remaining -= 1;
        let mut writer = bounded::RequestWriter(vec![]);
        serde_json::to_writer(&mut writer, value)
            .map_err(|_| Error::new(ErrorKind::RequestLimit, "bounded request serialization"))?;
        let response = self.transport.post(&Request {
            endpoint,
            body: writer.0,
        })?;
        let bytes = response.body.as_bytes();
        // BodyCollector already charges before each copy; check again at the
        // typed JSON boundary without trusting headers or a transport hint.
        if bytes.len() > MAX_BODY {
            return Err(Error::new(ErrorKind::BodyLimit, "HTTP body limit"));
        }
        self.receipts.push(HttpReceipt {
            endpoint,
            status: response.status,
            response_bytes: bytes.len(),
            response_sha256: hex::encode(crate::digest(bytes)),
        });
        if response.status == 429 {
            return Err(Error::new(
                ErrorKind::RateLimited,
                "provider HTTP rate limit",
            ));
        }
        if !(200..300).contains(&response.status) {
            return Err(Error::new(
                ErrorKind::Http,
                "provider HTTP status is not successful",
            ));
        }
        let json::UniqueValue(value) = serde_json::from_slice(bytes).map_err(|_| {
            Error::new(
                ErrorKind::Decode,
                "invalid or duplicate-field bounded provider JSON",
            )
        })?;
        match value.get("success").and_then(Value::as_bool) {
            Some(false) => {
                return Err(Error::new(
                    ErrorKind::ProviderRpc,
                    "provider reports success=false",
                ));
            }
            Some(true) => (),
            None => return Err(Error::new(ErrorKind::Decode, "missing boolean success")),
        }
        if ["error", "structuredError"]
            .iter()
            .any(|field| value.get(field).is_some_and(|error| !error.is_null()))
        {
            return Err(Error::new(
                ErrorKind::ProviderRpc,
                "successful envelope also carries error",
            ));
        }
        Ok(value)
    }
    fn network(&mut self) -> Result<()> {
        let value = self.call(Endpoint::GetNetworkInfo, &json!({}))?;
        let declared = value
            .get("genesis_challenge")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::new(ErrorKind::NetworkMismatch, "missing genesis challenge"))?;
        if value.get("network_name").and_then(Value::as_str) != Some("testnet11")
            || value.get("network_prefix").and_then(Value::as_str) != Some("txch")
            || hex32(declared)? != testnet11_network().genesis_challenge
        {
            return Err(Error::new(
                ErrorKind::NetworkMismatch,
                "provider is not pinned Testnet11",
            ));
        }
        Ok(())
    }
    fn tip(&mut self) -> Result<Tip> {
        let value = self.call(Endpoint::GetBlockchainState, &json!({}))?;
        let state = value
            .get("blockchain_state")
            .ok_or_else(|| Error::new(ErrorKind::Decode, "missing blockchain state"))?;
        let sync = &state["sync"];
        if sync["synced"] != true || sync["sync_mode"] != false {
            return Err(Error::new(ErrorKind::Incomplete, "provider is not synced"));
        }
        let peak: Peak = serde_json::from_value(state["peak"].clone())
            .map_err(|_| Error::new(ErrorKind::Decode, "missing or invalid peak"))?;
        Ok(Tip {
            height: peak.height,
            header_hash: hex32(&peak.header_hash)?,
        })
    }
    fn input(&mut self, expected: &CoinRecord, tip: &Tip) -> Result<InputState> {
        let value = self.call(
            Endpoint::GetCoinRecordByName,
            &json!({"name": prefixed(&expected.id())}),
        )?;
        let record = value.get("coin_record").ok_or_else(|| {
            Error::new(
                ErrorKind::Decode,
                "missing coin_record field is not absence",
            )
        })?;
        if record.is_null() {
            return Ok(InputState::Missing);
        }
        let record: WireRecord = serde_json::from_value(record.clone())
            .map_err(|_| Error::new(ErrorKind::Decode, "invalid coin record"))?;
        let actual = record.coin()?;
        if actual != *expected {
            return Err(Error::new(
                ErrorKind::InvalidRecord,
                "point lookup returned different coin",
            ));
        }
        record.state(tip)
    }
    fn children(
        &mut self,
        input: &CoinRecord,
        spent: u32,
        tip: &Tip,
    ) -> Result<Vec<ChildObservation>> {
        let mut children = vec![];
        let mut ids = BTreeSet::new();
        let mut cursors = BTreeSet::new();
        let mut cursor: Option<String> = None;
        let end_height = spent.checked_add(1).ok_or_else(|| {
            Error::new(ErrorKind::Incomplete, "spent-height query bound overflows")
        })?;
        for _ in 0..MAX_PAGES {
            let mut request = json!({"parent_ids": [prefixed(&input.id())], "include_spent_coins": true,
                "start_height": spent, "end_height": end_height, "limit": MAX_CHILDREN + 1, "order": "asc"});
            if let Some(value) = &cursor {
                request["cursor"] = json!(value);
            }
            let page = self.call(Endpoint::GetCoinRecordsByParentIds, &request)?;
            // The current Coinset schema documents truncated/next_cursor. This
            // adapter requires explicit completion; a silently clamped/omitted
            // marker cannot be interpreted as a complete child set.
            let truncated = page
                .get("truncated")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::Incomplete,
                        "explicit page completion marker required",
                    )
                })?;
            let records = page
                .get("coin_records")
                .and_then(Value::as_array)
                .ok_or_else(|| Error::new(ErrorKind::Decode, "missing coin_records array"))?;
            if records.len() > MAX_CHILDREN.saturating_sub(children.len()) {
                return Err(Error::new(
                    ErrorKind::Incomplete,
                    "child aggregate exceeds bounded snapshot",
                ));
            }
            for value in records {
                let record: WireRecord = serde_json::from_value(value.clone())
                    .map_err(|_| Error::new(ErrorKind::Decode, "invalid child record"))?;
                let coin = record.coin()?;
                record.state(tip)?;
                if coin.parent != input.id() || record.confirmed_block_index != spent {
                    return Err(Error::new(
                        ErrorKind::InvalidRecord,
                        "child parent/creation height differs from spent input",
                    ));
                }
                if !ids.insert(coin.id()) {
                    return Err(Error::new(
                        ErrorKind::Incomplete,
                        "duplicate child across provider pages",
                    ));
                }
                children.push(ChildObservation {
                    coin,
                    created_height: record.confirmed_block_index,
                });
            }
            let next = page.get("next_cursor");
            if !truncated {
                if next.is_some_and(|next| !next.is_null()) {
                    return Err(Error::new(
                        ErrorKind::Incomplete,
                        "complete page unexpectedly carries cursor",
                    ));
                }
                children.sort_by_key(|child| child.coin.id());
                return Ok(children);
            }
            let next = next
                .and_then(Value::as_str)
                .ok_or_else(|| Error::new(ErrorKind::Incomplete, "truncated page has no cursor"))?;
            if next.is_empty() || next.len() > MAX_CURSOR || !cursors.insert(next.to_owned()) {
                return Err(Error::new(
                    ErrorKind::Incomplete,
                    "invalid or repeated pagination cursor",
                ));
            }
            cursor = Some(next.to_owned());
        }
        Err(Error::new(
            ErrorKind::Incomplete,
            "parent pagination page budget",
        ))
    }
}

#[derive(Deserialize)]
struct Peak {
    height: u32,
    header_hash: String,
}
#[derive(Deserialize)]
struct WireCoin {
    parent_coin_info: String,
    puzzle_hash: String,
    amount: u64,
}
#[derive(Deserialize)]
struct WireRecord {
    coin: WireCoin,
    confirmed_block_index: u32,
    spent_block_index: u32,
    spent: bool,
    coinbase: bool,
    timestamp: u64,
}
impl WireRecord {
    fn coin(&self) -> Result<CoinRecord> {
        Ok(CoinRecord {
            parent: hex32(&self.coin.parent_coin_info)?,
            puzzle_hash: hex32(&self.coin.puzzle_hash)?,
            amount: self.coin.amount,
        })
    }
    fn state(&self, tip: &Tip) -> Result<InputState> {
        if self.confirmed_block_index > tip.height
            || self.spent
                && (self.spent_block_index < self.confirmed_block_index
                    || self.spent_block_index > tip.height)
            || !self.spent && self.spent_block_index != 0
        {
            return Err(Error::new(
                ErrorKind::InvalidRecord,
                "inconsistent coin heights",
            ));
        }
        // Parsed fields are provider declarations, not coinbase maturity or
        // authenticated wall-clock evidence. This observer spends nothing.
        let _ = (self.coinbase, self.timestamp);
        Ok(if self.spent {
            InputState::Spent {
                created_height: self.confirmed_block_index,
                spent_height: self.spent_block_index,
            }
        } else {
            InputState::Unspent {
                created_height: self.confirmed_block_index,
            }
        })
    }
}
fn prefixed(bytes: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(bytes))
}
fn hex32(value: &str) -> Result<[u8; 32]> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    let mut bytes = [0; 32];
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::new(
            ErrorKind::InvalidRecord,
            "expected exact 32-byte hex field",
        ));
    }
    hex::decode_to_slice(value, &mut bytes)
        .map_err(|_| Error::new(ErrorKind::InvalidRecord, "invalid hex field"))?;
    Ok(bytes)
}

/// Observe one exact declared coin. Only explicit null yields Missing; provider
/// errors, partial scans, unsteady tips and invalid records return Err instead.
/// Equal bracketing tips are a consistency heuristic, never chain proof.
pub fn observe<T: Transport>(transport: &mut T, expected: &CoinRecord) -> Result<Snapshot> {
    let mut session = Session {
        transport,
        remaining: MAX_CALLS,
        receipts: vec![],
    };
    session.network()?;
    let before = session.tip()?;
    let input = session.input(expected, &before)?;
    let children = match input {
        InputState::Spent { spent_height, .. } => {
            session.children(expected, spent_height, &before)?
        }
        _ => vec![],
    };
    let after = session.tip()?;
    if before != after {
        return Err(Error::new(
            ErrorKind::Incomplete,
            "tip changed during observation bracket",
        ));
    }
    Ok(Snapshot {
        observation: Observation {
            network: testnet11_network(),
            tip: before,
            input_id: expected.id(),
            input,
            children,
        },
        receipts: session.receipts,
        scope: "Bounded provider-declared complete child set and equal-tip consistency heuristic only; no authenticated inclusion, freshness, finality, consensus validation or transaction submission",
    })
}
