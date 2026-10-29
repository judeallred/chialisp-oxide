//! Fixed-origin Testnet11 submission and observation, separate from the original
//! read-only provider. Provider assertions and local signing constants are not
//! authenticated chain inclusion, genesis authentication, or finality proofs.
use chia_protocol::{Coin, CoinSpend, Program, SpendBundle};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Write, time::Duration};

// One crate-private parser serves both adapters; endpoints and transport
// capabilities remain separate, with no new public parser API.
use crate::provider::json::UniqueValue;

pub const ORIGIN: &str = "https://testnet11.api.coinset.org";
pub const MAX_PUSH_BYTES: usize = 512 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
pub const MAX_COINS: usize = 256;
const MAX_PAGES: usize = 16;
const MAX_RECEIPTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Transport,
    Timeout,
    Http,
    RateLimited,
    RequestLimit,
    ResponseLimit,
    Decode,
    InvalidRecord,
    NetworkMismatch,
    ProviderRpc,
    Inconclusive,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
fn error(kind: ErrorKind, message: &'static str) -> Error {
    Error {
        kind,
        message: message.into(),
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Endpoint {
    NetworkInfo,
    BlockchainState,
    CoinByName,
    CoinsByPuzzleHash,
    CoinsByParent,
    PushTransaction,
    MempoolByTransaction,
    PuzzleAndSolution,
}
impl Endpoint {
    fn path(self) -> &'static str {
        match self {
            Self::NetworkInfo => "get_network_info",
            Self::BlockchainState => "get_blockchain_state",
            Self::CoinByName => "get_coin_record_by_name",
            Self::CoinsByPuzzleHash => "get_coin_records_by_puzzle_hash",
            Self::CoinsByParent => "get_coin_records_by_parent_ids",
            Self::PushTransaction => "push_tx",
            Self::MempoolByTransaction => "get_mempool_item_by_tx_id",
            Self::PuzzleAndSolution => "get_puzzle_and_solution",
        }
    }
    fn response_limit(self) -> usize {
        match self {
            Self::NetworkInfo | Self::BlockchainState | Self::PushTransaction => 64 * 1024,
            Self::CoinByName => 128 * 1024,
            _ => MAX_RESPONSE_BYTES,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpReceipt {
    pub endpoint: Endpoint,
    pub http_status: u16,
    pub request_bytes: usize,
    pub request_sha256: String,
    pub response_bytes: usize,
    pub response_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkIdentity {
    pub provider_network_name: String,
    pub provider_network_prefix: String,
    pub origin: String,
    pub local_genesis_challenge: [u8; 32],
    pub local_agg_sig_me_domain: [u8; 32],
    pub scope: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveTip {
    pub height: u32,
    pub header_hash: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveCoinRecord {
    pub parent: [u8; 32],
    pub puzzle_hash: [u8; 32],
    pub amount: u64,
    pub confirmed_height: u32,
    pub spent_height: u32,
    pub spent: bool,
    pub coinbase: bool,
    pub timestamp: u64,
}
impl LiveCoinRecord {
    pub fn coin(&self) -> Coin {
        Coin::new(self.parent.into(), self.puzzle_hash.into(), self.amount)
    }
    pub fn id(&self) -> [u8; 32] {
        self.coin().coin_id().to_bytes()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoinSelection {
    pub records: Vec<LiveCoinRecord>,
    /// False means a bounded candidate set, never an entire-wallet balance.
    pub complete: bool,
    pub scope: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedSpend {
    pub parent: [u8; 32],
    pub puzzle_hash: [u8; 32],
    pub amount: u64,
    pub puzzle_reveal: Vec<u8>,
    pub solution: Vec<u8>,
}
impl ObservedSpend {
    pub fn coin(&self) -> Coin {
        Coin::new(self.parent.into(), self.puzzle_hash.into(), self.amount)
    }
    pub fn coin_spend(&self) -> CoinSpend {
        CoinSpend::new(
            self.coin(),
            Program::from(self.puzzle_reveal.clone()),
            Program::from(self.solution.clone()),
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum SubmissionOutcome {
    MempoolAccepted {
        transaction_id: [u8; 32],
    },
    Pending {
        transaction_id: [u8; 32],
    },
    /// Provider-reported rejection; not an independently checked consensus code.
    Rejected {
        transaction_id: [u8; 32],
        provider_code: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum MempoolOutcome {
    Present {
        transaction_id: [u8; 32],
        cost: u64,
        fee: u64,
    },
    Missing {
        transaction_id: [u8; 32],
    },
}

/// Synchronous client: construct/use/drop outside an entered Tokio runtime.
/// Production accepts no URL, headers, proxy, redirects, retries or signer.
pub struct Testnet11Client {
    client: reqwest::Client,
    runtime: Option<tokio::runtime::Runtime>,
    origin: reqwest::Url,
    receipts: Vec<HttpReceipt>,
    network_verified: bool,
}
impl Testnet11Client {
    pub fn new() -> Result<Self> {
        Self::build(
            ORIGIN,
            true,
            Duration::from_secs(5),
            Duration::from_secs(15),
        )
    }
    fn build(origin: &str, https: bool, connect: Duration, total: Duration) -> Result<Self> {
        outside_runtime()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| error(ErrorKind::Transport, "network runtime unavailable"))?;
        let client = runtime.block_on(async {
            reqwest::Client::builder()
                .use_rustls_tls()
                .https_only(https)
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .no_proxy()
                .connect_timeout(connect)
                .read_timeout(connect)
                .timeout(total)
                .pool_max_idle_per_host(1)
                .user_agent("chialisp-oxide/WIP-Testnet11")
                .build()
                .map_err(transport_error)
        })?;
        let origin = reqwest::Url::parse(origin)
            .map_err(|_| error(ErrorKind::Transport, "invalid fixed origin"))?;
        Ok(Self {
            client,
            runtime: Some(runtime),
            origin,
            receipts: Vec::new(),
            network_verified: false,
        })
    }
    pub fn receipts(&self) -> &[HttpReceipt] {
        &self.receipts
    }
    pub fn take_receipts(&mut self) -> Vec<HttpReceipt> {
        std::mem::take(&mut self.receipts)
    }
    fn call(&mut self, endpoint: Endpoint, request: &Value) -> Result<Value> {
        outside_runtime()?;
        if self.receipts.len() >= MAX_RECEIPTS {
            return Err(error(
                ErrorKind::Inconclusive,
                "HTTP receipt budget exhausted",
            ));
        }
        let request_limit = if endpoint == Endpoint::PushTransaction {
            MAX_PUSH_BYTES
        } else {
            4096
        };
        let mut body = BoundedWriter {
            bytes: Vec::new(),
            limit: request_limit,
        };
        serde_json::to_writer(&mut body, request)
            .map_err(|_| error(ErrorKind::RequestLimit, "bounded request serialization"))?;
        let request_bytes = body.bytes.len();
        let request_sha256 = hex::encode(Sha256::digest(&body.bytes));
        let url = self
            .origin
            .join(endpoint.path())
            .map_err(|_| error(ErrorKind::Transport, "fixed endpoint URL"))?;
        let limit = endpoint.response_limit();
        let (status, bytes) = self
            .runtime
            .as_ref()
            .ok_or_else(|| error(ErrorKind::Transport, "network runtime unavailable"))?
            .block_on(async {
                let mut response = self
                    .client
                    .post(url)
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .header(reqwest::header::ACCEPT, "application/json")
                    .body(body.bytes)
                    .send()
                    .await
                    .map_err(transport_error)?;
                if response.content_length().is_some_and(|n| n > limit as u64) {
                    return Err(error(
                        ErrorKind::ResponseLimit,
                        "declared response byte limit",
                    ));
                }
                let status = response.status().as_u16();
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
                    if chunk.len() > limit.saturating_sub(bytes.len()) {
                        return Err(error(
                            ErrorKind::ResponseLimit,
                            "streamed response byte limit",
                        ));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok((status, bytes))
            })?;
        self.receipts.push(HttpReceipt {
            endpoint,
            http_status: status,
            request_bytes,
            request_sha256,
            response_bytes: bytes.len(),
            response_sha256: hex::encode(Sha256::digest(&bytes)),
        });
        if status == 429 {
            return Err(error(
                ErrorKind::RateLimited,
                "provider rate limit; no automatic retry",
            ));
        }
        if !(200..300).contains(&status) {
            return Err(error(ErrorKind::Http, "unsuccessful provider HTTP status"));
        }
        let UniqueValue(value) = serde_json::from_slice(&bytes).map_err(|_| {
            error(
                ErrorKind::Decode,
                "invalid or duplicate-field provider JSON",
            )
        })?;
        if !value.is_object() || value.get("success").and_then(Value::as_bool).is_none() {
            return Err(error(ErrorKind::Decode, "missing exact boolean success"));
        }
        Ok(value)
    }
    pub fn verify_network(&mut self) -> Result<NetworkIdentity> {
        self.network_verified = false;
        let value = self.call(Endpoint::NetworkInfo, &json!({}))?;
        success(&value)?;
        if value["network_name"].as_str() != Some("testnet11")
            || value["network_prefix"].as_str() != Some("txch")
        {
            return Err(error(
                ErrorKind::NetworkMismatch,
                "provider network assertion differs from Testnet11",
            ));
        }
        let constants = &*chia_sdk_types::TESTNET11_CONSTANTS;
        self.network_verified = true;
        Ok(NetworkIdentity { provider_network_name: "testnet11".into(), provider_network_prefix: "txch".into(),
            origin: ORIGIN.into(), local_genesis_challenge: constants.genesis_challenge.to_bytes(),
            local_agg_sig_me_domain: constants.agg_sig_me_additional_data.to_bytes(),
            scope: "Fixed HTTPS provider network assertion plus pinned local Testnet11 signing constants; no authenticated genesis, chain inclusion or finality proof. Mutable build inputs and HTTPS provider are trusted.".into() })
    }
    pub fn synced_tip(&mut self) -> Result<LiveTip> {
        let value = self.call(Endpoint::BlockchainState, &json!({}))?;
        success(&value)?;
        let state = &value["blockchain_state"];
        if state["sync"]["synced"].as_bool() != Some(true)
            || state["sync"]["sync_mode"].as_bool() != Some(false)
        {
            return Err(error(
                ErrorKind::Inconclusive,
                "provider is not explicitly synced",
            ));
        }
        Ok(LiveTip {
            height: uint32(&state["peak"]["height"])?,
            header_hash: hash32(&state["peak"]["header_hash"])?,
        })
    }
    pub fn coin(&mut self, id: [u8; 32]) -> Result<Option<LiveCoinRecord>> {
        let value = self.call(Endpoint::CoinByName, &json!({"name": prefixed(id)}))?;
        success(&value)?;
        let record = value
            .get("coin_record")
            .ok_or_else(|| error(ErrorKind::Decode, "missing coin_record is not absence"))?;
        if record.is_null() {
            return Ok(None);
        }
        let record = record_from_value(record)?;
        if record.id() != id {
            return Err(error(
                ErrorKind::InvalidRecord,
                "point lookup coin identity mismatch",
            ));
        }
        Ok(Some(record))
    }
    /// Provider-reported exact confirmed spend bytes. The caller must compare
    /// every byte with its plan before deriving/point-querying expected children.
    /// This endpoint supplies no chain-authentication or general child-list proof.
    pub fn observed_spend(&mut self, id: [u8; 32], spent_height: u32) -> Result<ObservedSpend> {
        let value = self.call(
            Endpoint::PuzzleAndSolution,
            &json!({"coin_id": prefixed(id), "height": spent_height}),
        )?;
        success(&value)?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireSpend {
            coin: WireCoin,
            puzzle_reveal: String,
            solution: String,
        }
        let wire: WireSpend = serde_json::from_value(
            value
                .get("coin_solution")
                .ok_or_else(|| error(ErrorKind::Decode, "coin_solution required"))?
                .clone(),
        )
        .map_err(|_| error(ErrorKind::Decode, "exact typed coin solution required"))?;
        let result = ObservedSpend {
            parent: hash32(&json!(wire.coin.parent_coin_info))?,
            puzzle_hash: hash32(&json!(wire.coin.puzzle_hash))?,
            amount: wire.coin.amount,
            puzzle_reveal: program_bytes(&wire.puzzle_reveal)?,
            solution: program_bytes(&wire.solution)?,
        };
        if result.coin().coin_id().to_bytes() != id {
            return Err(error(
                ErrorKind::InvalidRecord,
                "observed spend coin identity mismatch",
            ));
        }
        Ok(result)
    }
    pub fn address_coins(&mut self, puzzle_hash: [u8; 32]) -> Result<CoinSelection> {
        let value = self.call(Endpoint::CoinsByPuzzleHash, &json!({"puzzle_hash": prefixed(puzzle_hash), "include_spent_coins": false, "limit": MAX_COINS+1}))?;
        success(&value)?;
        let records = records(&value)?;
        let mut ids = BTreeSet::new();
        for record in &records {
            if record.puzzle_hash != puzzle_hash || record.spent || !ids.insert(record.id()) {
                return Err(error(
                    ErrorKind::InvalidRecord,
                    "address candidate identity, spent state or duplication",
                ));
            }
        }
        if value.get("truncated").is_some_and(|v| !v.is_boolean())
            || value
                .get("next_cursor")
                .is_some_and(|v| !v.is_null() && !v.is_string())
        {
            return Err(error(
                ErrorKind::Decode,
                "invalid candidate completion metadata",
            ));
        }
        let complete = value.get("truncated").and_then(Value::as_bool) == Some(false)
            && value.get("next_cursor").is_none_or(Value::is_null);
        Ok(CoinSelection { records, complete, scope: "Bounded observed candidates; omitted completion metadata is not complete wallet inventory or balance. Reconfirm any selected exact coin before signing.".into() })
    }
    pub fn children(&mut self, parent: [u8; 32], spent_height: u32) -> Result<Vec<LiveCoinRecord>> {
        let end = spent_height
            .checked_add(1)
            .ok_or_else(|| error(ErrorKind::Inconclusive, "child query height overflow"))?;
        let mut result = Vec::new();
        let mut ids = BTreeSet::new();
        let mut cursors = BTreeSet::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut request = json!({"parent_ids": [prefixed(parent)], "include_spent_coins": true, "start_height": spent_height, "end_height": end, "limit": MAX_COINS+1, "order": "asc"});
            if let Some(cursor) = &cursor {
                request["cursor"] = json!(cursor);
            }
            let value = self.call(Endpoint::CoinsByParent, &request)?;
            success(&value)?;
            let truncated = value
                .get("truncated")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    error(
                        ErrorKind::Inconclusive,
                        "provider omits explicit child-list completion",
                    )
                })?;
            let page = records(&value)?;
            if page.len() > MAX_COINS.saturating_sub(result.len()) {
                return Err(error(
                    ErrorKind::Inconclusive,
                    "aggregate child count bound",
                ));
            }
            for record in page {
                if record.parent != parent
                    || record.confirmed_height != spent_height
                    || !ids.insert(record.id())
                {
                    return Err(error(
                        ErrorKind::InvalidRecord,
                        "child identity, creation height or duplicate mismatch",
                    ));
                }
                result.push(record);
            }
            let next = value.get("next_cursor");
            if !truncated {
                if next.is_some_and(|n| !n.is_null()) {
                    return Err(error(
                        ErrorKind::Inconclusive,
                        "complete page carries cursor",
                    ));
                }
                result.sort_by_key(LiveCoinRecord::id);
                return Ok(result);
            }
            let next = next
                .and_then(Value::as_str)
                .ok_or_else(|| error(ErrorKind::Inconclusive, "truncated page has no cursor"))?;
            if next.is_empty()
                || next.len() > 256
                || next.bytes().any(|b| b.is_ascii_control())
                || !cursors.insert(next.to_owned())
            {
                return Err(error(ErrorKind::Inconclusive, "invalid or repeated cursor"));
            }
            cursor = Some(next.to_owned());
        }
        Err(error(
            ErrorKind::Inconclusive,
            "child pagination page budget",
        ))
    }
    /// Explicit submission only. A timeout/HTTP/decode error is inconclusive:
    /// the node may have received it. Never automatically retry or call it rejected.
    pub fn push(&mut self, bundle: &SpendBundle) -> Result<SubmissionOutcome> {
        if !self.network_verified {
            return Err(error(
                ErrorKind::NetworkMismatch,
                "verify Testnet11 provider assertion before submission",
            ));
        }
        let request = bundle_request(bundle)?;
        let transaction_id = bundle.name().to_bytes();
        let value = self.call(Endpoint::PushTransaction, &request)?;
        reported_ids(&value, transaction_id)?;
        match (value["success"].as_bool(), value["status"].as_str()) {
            (Some(true), Some("SUCCESS")) => {
                success(&value)?;
                Ok(SubmissionOutcome::MempoolAccepted { transaction_id })
            }
            (Some(true), Some("PENDING")) => {
                success(&value)?;
                Ok(SubmissionOutcome::Pending { transaction_id })
            }
            (Some(false), Some("PENDING")) => {
                // A pending failure is distinct from admission, but its error
                // fields still need one bounded, consistent provider code.
                provider_code(&value)?;
                Ok(SubmissionOutcome::Pending { transaction_id })
            }
            (Some(false), Some("FAILED")) => Ok(SubmissionOutcome::Rejected {
                transaction_id,
                provider_code: provider_code(&value)?,
            }),
            _ => Err(error(
                ErrorKind::Inconclusive,
                "unrecognized submission outcome; reconcile exact transaction",
            )),
        }
    }
    pub fn mempool(&mut self, transaction_id: [u8; 32]) -> Result<MempoolOutcome> {
        let value = self.call(
            Endpoint::MempoolByTransaction,
            &json!({"tx_id": prefixed(transaction_id)}),
        )?;
        reported_ids(&value, transaction_id)?;
        if value["success"].as_bool() == Some(false)
            && value["structuredError"]["code"].as_str() == Some("TX_NOT_IN_MEMPOOL")
        {
            if hash32(&value["structuredError"]["data"]["tx_id"])? != transaction_id {
                return Err(error(
                    ErrorKind::InvalidRecord,
                    "missing mempool transaction identity mismatch",
                ));
            }
            // The actual provider supplies a generic message alongside its
            // structured code. Admit only the matching point-query message;
            // conflicting error/status/item fields cannot manufacture Missing.
            let expected = format!("Tx {} not in mempool", prefixed(transaction_id));
            if value
                .get("error")
                .is_some_and(|v| !v.is_null() && v.as_str() != Some(expected.as_str()))
                || value.get("status").is_some_and(|v| !v.is_null())
                || value.get("mempool_item").is_some_and(|v| !v.is_null())
                || value["structuredError"]
                    .get("message")
                    .is_some_and(|v| v.as_str() != Some("Tx not in mempool"))
                || value.get("traceback").is_some_and(|v| !v.is_null())
            {
                return Err(error(
                    ErrorKind::InvalidRecord,
                    "contradictory missing-mempool envelope",
                ));
            }
            return Ok(MempoolOutcome::Missing { transaction_id });
        }
        success(&value)?;
        if value
            .get("status")
            .is_some_and(|v| !v.is_null() && v.as_str() != Some("SUCCESS"))
        {
            return Err(error(
                ErrorKind::InvalidRecord,
                "contradictory present-mempool status",
            ));
        }
        let item = &value["mempool_item"];
        reported_ids(item, transaction_id)?;
        if hash32(&item["spend_bundle_name"])? != transaction_id {
            return Err(error(
                ErrorKind::InvalidRecord,
                "mempool transaction identity mismatch",
            ));
        }
        Ok(MempoolOutcome::Present {
            transaction_id,
            cost: uint64(&item["cost"])?,
            fee: uint64(&item["fee"])?,
        })
    }
}
impl Drop for Testnet11Client {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}
fn outside_runtime() -> Result<()> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(error(
            ErrorKind::Transport,
            "synchronous network client requires an ordinary thread",
        ));
    }
    Ok(())
}
fn transport_error(e: reqwest::Error) -> Error {
    if e.is_timeout() {
        error(
            ErrorKind::Timeout,
            "HTTP deadline; submission may have arrived, no automatic retry",
        )
    } else {
        error(
            ErrorKind::Transport,
            "HTTP transport failed; submission may have arrived, no automatic retry",
        )
    }
}
fn success(value: &Value) -> Result<()> {
    if value["success"].as_bool() != Some(true) {
        return Err(error(
            ErrorKind::ProviderRpc,
            "provider reports unsuccessful RPC",
        ));
    }
    if ["error", "structuredError"]
        .iter()
        .any(|key| value.get(key).is_some_and(|v| !v.is_null()))
    {
        return Err(error(
            ErrorKind::ProviderRpc,
            "successful envelope also carries error",
        ));
    }
    Ok(())
}
fn uint64(v: &Value) -> Result<u64> {
    v.as_u64()
        .ok_or_else(|| error(ErrorKind::Decode, "exact unsigned integer required"))
}
fn uint32(v: &Value) -> Result<u32> {
    u32::try_from(uint64(v)?).map_err(|_| error(ErrorKind::Decode, "u32 field overflow"))
}
fn hash32(v: &Value) -> Result<[u8; 32]> {
    let s = v
        .as_str()
        .ok_or_else(|| error(ErrorKind::Decode, "32-byte hex string required"))?;
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() != 64 || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(error(
            ErrorKind::Decode,
            "exact 32-byte hexadecimal required",
        ));
    }
    let mut bytes = [0; 32];
    hex::decode_to_slice(s, &mut bytes)
        .map_err(|_| error(ErrorKind::Decode, "invalid hex field"))?;
    Ok(bytes)
}
fn prefixed(id: [u8; 32]) -> String {
    format!("0x{}", hex::encode(id))
}
fn reported_ids(value: &Value, expected: [u8; 32]) -> Result<()> {
    for key in ["transaction_id", "tx_id", "spend_bundle_name"] {
        if let Some(id) = value.get(key)
            && hash32(id)? != expected
        {
            return Err(error(
                ErrorKind::InvalidRecord,
                "reported transaction identity mismatch",
            ));
        }
    }
    Ok(())
}
fn program_bytes(value: &str) -> Result<Vec<u8>> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.is_empty()
        || value.len() > MAX_PUSH_BYTES
        || !value.len().is_multiple_of(2)
        || !value.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(error(
            ErrorKind::Decode,
            "bounded serialized program hexadecimal required",
        ));
    }
    hex::decode(value)
        .map_err(|_| error(ErrorKind::Decode, "invalid serialized program hexadecimal"))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCoin {
    parent_coin_info: String,
    puzzle_hash: String,
    amount: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRecord {
    coin: WireCoin,
    confirmed_block_index: u32,
    spent_block_index: u32,
    spent: bool,
    coinbase: bool,
    timestamp: u64,
}
fn record_from_value(v: &Value) -> Result<LiveCoinRecord> {
    let wire: WireRecord = serde_json::from_value(v.clone())
        .map_err(|_| error(ErrorKind::Decode, "exact typed coin record required"))?;
    // A creation at height zero is representable. A reported spend at height
    // zero is not: zero is the wire format's unspent-height sentinel, and this
    // live workflow does not admit a contradictory spent=true sentinel.
    if (wire.spent
        && (wire.spent_block_index == 0 || wire.spent_block_index < wire.confirmed_block_index))
        || (!wire.spent && wire.spent_block_index != 0)
    {
        return Err(error(ErrorKind::InvalidRecord, "inconsistent spent height"));
    }
    Ok(LiveCoinRecord {
        parent: hash32(&json!(wire.coin.parent_coin_info))?,
        puzzle_hash: hash32(&json!(wire.coin.puzzle_hash))?,
        amount: wire.coin.amount,
        confirmed_height: wire.confirmed_block_index,
        spent_height: wire.spent_block_index,
        spent: wire.spent,
        coinbase: wire.coinbase,
        timestamp: wire.timestamp,
    })
}
fn records(value: &Value) -> Result<Vec<LiveCoinRecord>> {
    let rows = value["coin_records"]
        .as_array()
        .ok_or_else(|| error(ErrorKind::Decode, "coin_records array required"))?;
    if rows.len() > MAX_COINS {
        return Err(error(ErrorKind::Inconclusive, "coin candidate count bound"));
    }
    rows.iter().map(record_from_value).collect()
}
fn provider_code(value: &Value) -> Result<String> {
    let generic = value.get("error").filter(|v| !v.is_null());
    let structured = value.get("structuredError").filter(|v| !v.is_null());
    let first = generic
        .map(|v| {
            v.as_str()
                .ok_or_else(|| error(ErrorKind::Inconclusive, "invalid generic provider code"))
        })
        .transpose()?;
    let second = structured
        .map(|v| {
            v["code"]
                .as_str()
                .ok_or_else(|| error(ErrorKind::Inconclusive, "invalid structured provider code"))
        })
        .transpose()?;
    if first.zip(second).is_some_and(|(a, b)| a != b) {
        return Err(error(
            ErrorKind::Inconclusive,
            "conflicting provider error codes",
        ));
    }
    let code = first.or(second).ok_or_else(|| {
        error(
            ErrorKind::Inconclusive,
            "failed submission lacks provider code",
        )
    })?;
    if code.is_empty()
        || code.len() > 128
        || !code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(error(
            ErrorKind::Inconclusive,
            "failed submission has unrecognized provider code",
        ));
    }
    Ok(code.to_owned())
}
fn bundle_request(bundle: &SpendBundle) -> Result<Value> {
    // Charge all expanded hexadecimal strings and per-input overhead before
    // allocating any of them, then serialize through the exact bounded writer.
    if bundle.coin_spends.is_empty() || bundle.coin_spends.len() > 32 {
        return Err(error(
            ErrorKind::RequestLimit,
            "bounded nonempty spend inventory required",
        ));
    }
    let mut remaining = MAX_PUSH_BYTES - 512;
    let mut ids = BTreeSet::new();
    for spend in &bundle.coin_spends {
        let size = spend
            .puzzle_reveal
            .len()
            .checked_add(spend.solution.len())
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(512))
            .ok_or_else(|| error(ErrorKind::RequestLimit, "bundle framing overflow"))?;
        if size > remaining || !ids.insert(spend.coin.coin_id()) {
            return Err(error(
                ErrorKind::RequestLimit,
                "bundle byte bound or duplicate input",
            ));
        }
        remaining -= size;
    }
    Ok(
        json!({"spend_bundle": {"coin_spends": bundle.coin_spends.iter().map(|s| json!({"coin": {"parent_coin_info": prefixed(s.coin.parent_coin_info.to_bytes()), "puzzle_hash": prefixed(s.coin.puzzle_hash.to_bytes()), "amount": s.coin.amount}, "puzzle_reveal": format!("0x{}",hex::encode(s.puzzle_reveal.as_slice())), "solution": format!("0x{}",hex::encode(s.solution.as_slice()))})).collect::<Vec<_>>(), "aggregated_signature": format!("0x{}",hex::encode(bundle.aggregated_signature.to_bytes()))}}),
    )
}
struct BoundedWriter {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("request byte bound"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
