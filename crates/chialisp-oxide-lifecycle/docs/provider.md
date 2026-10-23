# Bounded provider observation foundation

WIP prototype, not ready for use. `provider` implements a transport-neutral,
read-only Testnet11 observation assembler, fixed-origin HTTPS backend and local
mock/loopback controls. It has no wallet integration, polling service, transaction push or live
confirmation evidence. Public provider declarations are not authenticated chain
inclusion, freshness or finality. It does not complete issue #12 or #13.

`observe(&mut transport, &exact_coin)` checks the provider's network name,
address prefix and exact pinned Testnet11 genesis challenge, reads a synced peak,
looks up the exact declared coin ID, and brackets the observation with the same
height and header hash. A changed tip, including a lower height or same-height
different hash, returns `Incomplete`. Equal tips are only a provider consistency
heuristic: independent HTTP reads do not become an atomic chain snapshot.

Only a successful response with explicit `coin_record: null` yields `Missing`.
Missing fields, `success:false`, HTTP errors and malformed records are errors.
Every returned coin is checked against its complete parent/hash/amount identity
using the pinned SDK's coin-ID calculation. Height contradictions fail. This
observer does not establish coinbase maturity or local spend validity.

For a spent input, all children are queried by parent ID with spent children
included. Looking up only expected children would miss unexpected outputs.
Unknown extra children are retained for the journal's conflict check. This
adapter requires an explicit boolean `truncated` on every page: absence does
not prove completeness even when a requested `limit` was silently clamped.
Truncated pages require a nonempty, unrepeated cursor of at most 256 bytes.
Completion must be explicit, with no continuation cursor. More than 256 total
children, duplicate children or more than 16 pages fail rather than trim data.
Child parents and creation heights must match the actual input and spent height.
No partial scan produces an `Observation`.

The closed `Endpoint` enum contains only four read RPCs. Each session reserves
one of twenty request/response allowances before serialization or transport;
requests serialize through a writer capped at 4 KiB. Responses can only be
constructed through `BodyCollector`, which checks remaining 64-KiB capacity
before extending its buffer and remains failed after overflow. JSON parsing
comes after that cap and rejects duplicate object keys at every nesting level,
before a generic JSON object can silently overwrite a critical value. A
`success:true` envelope carrying nonnull `error` or `structuredError` is rejected.
Per snapshot the allowances total at most 80 KiB of
requests and 1.25 MiB of responses. Local JSON parsing additionally retains its
default nesting limit. Network strings and records are bounded by the envelope;
pagination cursors are checked before request copies. Successful snapshots
retain endpoint/status/body-length/body-digest receipts; these are consistency
metadata, not signatures or authenticated execution evidence.

`Testnet11Https::new()` accepts no URL or caller-selected TLS/header options.
It uses exact reqwest 0.12.28 with Rustls/WebPKI roots, verified TLS, HTTPS-only
requests, no proxy or redirects, and finite connection/read/overall deadlines
of 5/5/15 seconds. It awaits response chunks into `BodyCollector`, with no
unrestricted JSON/text/body collection. Content-Length can reject an oversized
body early but never grants an allocation allowance or completeness evidence.
The total deadline remains attached during body reads, even when each chunk
arrives within the read deadline. It exposes only the closed read endpoints.

The synchronous backend owns a current-thread async runtime. Construct and call
it from an ordinary dedicated thread outside an entered Tokio runtime. It
conservatively rejects entered-runtime calls, including runtime-associated
blocking workers; an ordinary thread/queue bridge is required for future GUI
integration. Dropping an idle backend in an async context uses nonblocking
runtime shutdown instead of panicking. No GUI adapter currently calls it. Polling
requires its own finite session budget, backoff and operation permit. The trait
cannot enforce wall-clock behavior of an arbitrary implementation and is not a
sandbox. Errors distinguish transport, timeout, HTTP/rate limits, body/request
limits, decoding, provider RPC declarations, network mismatch, invalid records
and incomplete observations. A provider-reported code is never promoted to
local typed consensus rejection.

The first local public-recipient workflow should consume a private validated
fresh recipe result, commit a real recipient before exporting its funding
request, and preserve that exact reveal and output in the journal. Historical
`0x42` and simulator `0x23` destinations are fixtures. A wallet signs funding
externally; no key, mnemonic or signer secret belongs in this provider. The
current permissionless payment returns the full 1,000 mojos, so its fee is zero.
Actual condition and signature validation remains separate even when an empty
aggregate is correct. An arbitrary saved report or caller-supplied check event
cannot grant fresh execution credit. No push API is exposed in this slice.

Contracts were checked against Coinset's [official API explorer](https://www.coinset.org/docs/api?api=full-node)
and its [published schema](https://www.coinset.org/openapi/full_node_bundled.json),
retrieved October 22, 2026: 53,708 bytes, SHA-256
`3f34f9d1eacfccb20db4ff0c6a6d2d0e2b5428e0e8e727e7b9bda89bc8166783`.
That mutable schema documents pagination and an optional completion marker;
this adapter intentionally rejects omission. The pinned SDK source is
[`chia-sdk-coinset 0.36.0`](https://github.com/xch-dev/chia-wallet-sdk/tree/e9a08f6e54ea5132f7da06327836a3b386a944b1/crates/chia-sdk-coinset),
Apache-2.0. Its stock HTTP client has no configured timeout/body cap and is not
used here. The dependency addition preserves every previously locked package
version. TLS adds Rustls 0.23.46, ring 0.17.14, hyper-rustls 0.27.10,
tokio-rustls 0.26.6 and WebPKI roots 1.0.10 plus their locked support packages;
the resolved feature inventory is reviewed separately from lock-only optional
and non-host packages. It does not enable reqwest blocking, HTTP3, compression,
cookies or an arbitrary URL API.

Tests use constructed transport/record controls and actual loopback HTTP. The
private test-only client can use an ephemeral loopback HTTP origin and shorter
deadlines. Controls cover chunked bodies with no length, stream/declared body
overflow, headers/body/read and total deadlines, HTTP statuses, redirect refusal,
and entered-runtime rejection without public network calls. They do not verify
a live TLS connection, authenticated chain data or new VM/SDK transactions.
Execution status is reported separately after coordinated native gates.

The HTTPS client disables automatic retries, including protocol NACK retries,
so one bounded assembler call permits one wire attempt.
