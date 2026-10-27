"use strict";
(() => {
  const ACTIONS = Object.freeze(["reload", "prepare_funding", "submit_funding", "monitor_funding", "prepare_payment", "submit_payment", "monitor_payment"]);
  const STATES = Object.freeze(["missing", "unspent", "confirmed", "conflict", "inconclusive"]);
  const MAX_RESPONSE = 128 * 1024;
  const hash = (value) => typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
  function requireValue(condition, message) { if (!condition) throw new Error(message); }
  function decimal(value) {
    requireValue(typeof value === "string" && /^(?:0|[1-9][0-9]{0,39})$/.test(value), "An exact unsigned decimal string is required.");
    return BigInt(value);
  }
  function integer(value) { return decimal(value).toLocaleString("en-US"); }
  function txch(value) {
    const amount = decimal(value);
    const whole = amount / 1000000000000n;
    const tail = (amount % 1000000000000n).toString().padStart(12, "0").replace(/0+$/, "");
    return `${whole.toLocaleString("en-US")}${tail ? `.${tail}` : ""} TXCH`;
  }
  function mojos(value) { return `${integer(value)} mojos`; }
  function text(value, maximum = 512) {
    requireValue(typeof value === "string" && value.length <= maximum, "Invalid public text field.");
    return value;
  }
  function admitCoin(coin) {
    requireValue(coin && hash(coin.coin_id) && hash(coin.parent) && hash(coin.puzzle_hash), "Invalid public coin identity.");
    decimal(coin.amount_mojos);
  }
  function admitTransfer(transfer) {
    requireValue(transfer && typeof transfer.prepared === "boolean" && STATES.includes(transfer.state), "Invalid transfer state.");
    requireValue(Array.isArray(transfer.inputs) && transfer.inputs.length <= 2 && Array.isArray(transfer.outputs) && transfer.outputs.length <= 3, "Invalid bounded coin inventory.");
    for (const rows of [transfer.inputs, transfer.outputs]) {
      rows.forEach(admitCoin);
      requireValue(new Set(rows.map((coin) => coin.coin_id)).size === rows.length, "Duplicate public coin identity.");
    }
    if (transfer.prepared) {
      requireValue(hash(transfer.bundle_id) && hash(transfer.bundle_sha256) && transfer.inputs.length > 0 && transfer.outputs.length > 0, "Incomplete prepared transfer.");
      decimal(transfer.preflight_height);
      requireValue(transfer.cost, "Prepared transfer has no retained cost.");
    }
    if (transfer.cost !== null) {
      requireValue(transfer.cost && typeof transfer.cost === "object", "Invalid cost projection.");
      for (const key of ["total", "execution", "conditions", "charged_generator_bytes", "byte_cost"]) decimal(transfer.cost[key]);
      requireValue(decimal(transfer.cost.execution) + decimal(transfer.cost.conditions) + decimal(transfer.cost.byte_cost) === decimal(transfer.cost.total), "Retained cost components disagree.");
    }
    if (transfer.provider_outcome !== null) text(transfer.provider_outcome);
    if (transfer.latest_observation !== null) {
      decimal(transfer.latest_observation.tip_height);
      if (transfer.latest_observation.spent_height !== null) decimal(transfer.latest_observation.spent_height);
      text(transfer.latest_observation.scope, 2048);
    }
  }
  function admitView(view) {
    requireValue(view && typeof view === "object" && view.network === "testnet11" && hash(view.revision), "An exact Testnet11 view and saved revision are required.");
    text(view.protocol, 128);
    for (const name of ["wallet_puzzle_hash", "funding_puzzle_hash", "reveal_sha256"]) requireValue(hash(view[name]), "Invalid checked program or wallet hash.");
    requireValue(["on", "off"].includes(view.mode), "Invalid Rust mode.");
    text(view.fresh_execution, 128);
    for (const name of ["amount_mojos", "funding_fee_mojos", "payment_fee_mojos"]) decimal(view[name]);
    requireValue(view.amount_mojos === "1000", "Unexpected fixed payment amount.");
    requireValue(Number.isSafeInteger(view.event_count) && view.event_count >= 0 && view.event_count <= 32 && Number.isSafeInteger(view.remaining_events) && view.remaining_events >= 0 && view.remaining_events <= 32 && view.event_count + view.remaining_events === 32, "Invalid public event capacity.");
    requireValue(Array.isArray(view.available_actions) && view.available_actions.length <= ACTIONS.length && view.available_actions.every((action) => ACTIONS.includes(action)) && new Set(view.available_actions).size === view.available_actions.length, "Invalid available action list.");
    text(view.scope, 4096);
    admitTransfer(view.funding); admitTransfer(view.payment);
    return view;
  }
  function admitJob(job) {
    if (job === null) return;
    requireValue(job && ACTIONS.includes(job.action) && ["queued", "running", "succeeded", "failed"].includes(job.status), "Invalid local job state.");
    decimal(job.id);
    if (job.error !== undefined && job.error !== null) { text(job.error.kind, 128); text(job.error.message); }
  }
  function actionBody(action, revision) {
    requireValue(ACTIONS.includes(action) && hash(revision), "Closed action and exact revision required.");
    return { action, revision };
  }
  // Pure test access never contains a session capability or per-user state.
  if (typeof document === "undefined") {
    globalThis.oxideLiveFormatters = Object.freeze({ decimal, integer, txch, mojos, admitView, admitJob, actionBody });
    return;
  }
  const capability = document.querySelector('meta[name="oxide-session"]')?.content;
  const byId = (id) => document.getElementById(id);
  const buttons = Array.from(document.querySelectorAll("[data-action]"));
  let current = null;
  let requestInFlight = false;
  let readInFlight = false;
  let localNeedsReload = false;
  let pendingJobId = null;
  let activeAction = null;
  let timer = null;
  const actionNames = Object.freeze({ reload: "Reload checked state", prepare_funding: "Prepare and sign funding", submit_funding: "Submit saved funding", monitor_funding: "Monitor funding", prepare_payment: "Prepare payment", submit_payment: "Submit saved payment", monitor_payment: "Monitor payment" });
  const set = (id, value) => { byId(id).textContent = value; };
  function showError(kind, message) {
    byId("error").hidden = false;
    byId("error").textContent = `${kind}: ${message}`;
  }
  function clearError() { byId("error").hidden = true; byId("error").textContent = ""; }
  function busy() {
    return requestInFlight || pendingJobId !== null || (current?.job && ["queued", "running"].includes(current.job.status));
  }
  function needsReload() { return localNeedsReload || current?.needs_reload === true; }
  function node(tag, className, content) {
    const element = document.createElement(tag);
    if (className) element.className = className;
    if (content !== undefined) element.textContent = content;
    return element;
  }
  function coinList(id, coins, kind) {
    const rows = coins.map((coin, index) => {
      const box = node("section", "coin");
      const title = node("p", "coin-title");
      title.append(node("span", "", `${kind} ${index + 1}`), node("span", "", mojos(coin.amount_mojos)));
      box.append(title);
      if (coin.puzzle_hash === current.view.wallet_puzzle_hash) box.append(node("p", "coin-role", "This wallet · payment or change"));
      else if (coin.puzzle_hash === current.view.funding_puzzle_hash) box.append(node("p", "coin-role", "Checked Rust puzzle"));
      const values = node("dl");
      for (const [label, value] of [["Coin ID", coin.coin_id], ["Parent coin ID", coin.parent], ["Puzzle hash", coin.puzzle_hash]]) values.append(node("dt", "", label), node("dd", "hash", value));
      box.append(values);
      return box;
    });
    byId(id).replaceChildren(...rows);
  }
  function transferView(kind, transfer) {
    const labels = { missing: transfer.prepared ? "Not observed" : "Not prepared", unspent: "Inputs unspent", confirmed: "Confirmed*", conflict: "Conflict", inconclusive: "Inconclusive" };
    const tone = { missing: "neutral", unspent: "pending", confirmed: "ready", conflict: "failure", inconclusive: "pending" };
    set(`${kind}-state`, labels[transfer.state]);
    byId(`${kind}-state`).className = `badge ${tone[transfer.state]}`;
    byId(`${kind}-details`).hidden = !transfer.prepared;
    const outcome = transfer.provider_outcome ? `Provider result: ${transfer.provider_outcome}. ` : "";
    set(`${kind}-outcome`, transfer.prepared ? `${outcome}Only the exact saved signed bundle can be submitted. Monitoring is explicit.` : (kind === "funding" ? "No funding bundle is prepared." : "Observe confirmed funding before preparing the payment."));
    if (transfer.prepared) {
      set(`${kind}-bundle-id`, transfer.bundle_id);
      set(`${kind}-bundle-sha256`, transfer.bundle_sha256);
      set(`${kind}-preflight-height`, integer(transfer.preflight_height));
      for (const [suffix, field] of [["total", "total"], ["execution", "execution"], ["conditions", "conditions"], ["bytes", "charged_generator_bytes"], ["byte-cost", "byte_cost"]]) set(`${kind}-cost-${suffix}`, integer(transfer.cost[field]));
      coinList(`${kind}-inputs`, transfer.inputs, "Input");
      coinList(`${kind}-outputs`, transfer.outputs, "Output");
    }
    const observation = transfer.latest_observation;
    set(`${kind}-observation`, observation ? `Retained provider tip: ${integer(observation.tip_height)}. ${observation.spent_height === null ? "No spent height recorded." : `Recorded spent height: ${integer(observation.spent_height)}.`}\n${observation.scope}\nThis is the last saved observation. Choose Monitor for a new provider query.` : `None recorded. Choose Monitor ${kind} to query the provider.`);
  }
  function render() {
    const view = current?.view;
    const job = current?.job;
    const working = busy();
    const fresh = view?.fresh_execution === "executed_this_process";
    const available = new Set(view?.available_actions || []);
    for (const button of buttons) {
      const action = button.dataset.action;
      button.disabled = working || !view || (action !== "reload" && (needsReload() || !fresh || !available.has(action)));
    }
    const jobTerminal = !working && job && ["succeeded", "failed"].includes(job.status);
    const indicator = byId("job-indicator");
    indicator.className = `status-dot${working ? " busy" : (job?.status === "failed" || needsReload() ? " error" : " success")}`;
    set("job-status", working ? "Local job in progress" : (needsReload() ? "Reload required" : (jobTerminal ? (job.status === "succeeded" ? "Local job complete" : "Local job failed") : "Ready")));
    set("job-detail", working ? `${actionNames[activeAction || job?.action] || "Waiting for the accepted job"}${job?.id ? ` · job ${integer(job.id)}` : ""}. No second action is queued.` : (needsReload() ? "Reload checked state before another action. Nothing is automatically resubmitted." : "Opening or polling this page does not query the chain or submit a transaction."));
    const terminalAction = activeAction || job?.action;
    byId("terminal-note").hidden = !(working && ["prepare_funding", "prepare_payment"].includes(terminalAction));
    if (!view) return;
    set("connection-status", needsReload() ? "Cached view retained; Reload is required to reconcile durable state." : "Cached public view is responsive · provider monitoring is explicit");
    set("payment-amount", mojos(view.amount_mojos)); set("payment-txch", txch(view.amount_mojos));
    set("source-mode", view.mode === "on" ? "Optimization on" : "Optimization off");
    set("source-status", fresh ? "Fresh checks · this process" : "Fresh execution unavailable");
    byId("source-status").className = `badge ${fresh ? "ready" : "failure"}`;
    set("funding-fee", `${mojos(view.funding_fee_mojos)} · ${txch(view.funding_fee_mojos)}`);
    set("payment-fee", `${mojos(view.payment_fee_mojos)} · ${txch(view.payment_fee_mojos)}`);
    set("event-capacity", `${view.event_count} saved · ${view.remaining_events} remaining`);
    set("wallet-puzzle-hash", view.wallet_puzzle_hash); set("funding-puzzle-hash", view.funding_puzzle_hash);
    set("reveal-sha256", view.reveal_sha256); set("revision", view.revision); set("verification-scope", view.scope);
    transferView("funding", view.funding); transferView("payment", view.payment);
  }
  async function responseValue(response) {
    const length = response.headers.get("content-length");
    requireValue(length === null || (/^[0-9]+$/.test(length) && BigInt(length) <= BigInt(MAX_RESPONSE)), "Oversized local response.");
    requireValue(response.body, "Empty local response.");
    const reader = response.body.getReader(); const chunks = []; let count = 0;
    try {
      while (true) {
        const { done, value } = await reader.read(); if (done) break;
        requireValue(value.byteLength <= MAX_RESPONSE - count, "Oversized local response.");
        count += value.byteLength; chunks.push(value);
      }
    } catch (failure) { await reader.cancel().catch(() => {}); throw failure; }
    const bytes = new Uint8Array(count); let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  }
  async function request(path, body) {
    requireValue(path === "/api/state" || path === "/api/action", "Closed local endpoint required.");
    const headers = { "X-Oxide-Session": capability };
    if (body !== undefined) headers["Content-Type"] = "application/json";
    const response = await fetch(path, { method: body === undefined ? "GET" : "POST", headers, body: body === undefined ? undefined : JSON.stringify(body), mode: "same-origin", credentials: "omit", cache: "no-store", signal: AbortSignal.timeout(6000) });
    const result = await responseValue(response);
    if (!response.ok) {
      const supplied = result?.error;
      throw { kind: typeof supplied?.kind === "string" ? supplied.kind.slice(0, 128) : "request_failed", message: typeof supplied?.message === "string" ? supplied.message.slice(0, 512) : "The local request failed. Reload checked state before another action." };
    }
    return result;
  }
  function schedule(delay) { if (timer !== null) clearTimeout(timer); timer = setTimeout(poll, delay); }
  async function poll() {
    if (readInFlight) { schedule(1000); return; }
    readInFlight = true;
    try {
      const value = await request("/api/state");
      admitView(value.view); admitJob(value.job);
      requireValue(typeof value.needs_reload === "boolean", "Missing durable-state reconciliation flag.");
      current = value;
      if (pendingJobId !== null && value.job !== null && decimal(value.job.id) >= decimal(pendingJobId)) pendingJobId = null;
      if (value.job?.action === "reload" && value.job.status === "succeeded" && !value.needs_reload) { localNeedsReload = false; clearError(); }
      if (value.job?.status === "failed") showError(value.job.error?.kind || "job_failed", value.job.error?.message || "The local job failed. Reload checked state before another action.");
      if (!busy()) activeAction = null;
    } catch (failure) {
      localNeedsReload = true;
      set("connection-status", "The cached local view could not be read. Reload the page if the server session changed.");
      showError(failure?.kind || "state_unavailable", failure?.message && failure.kind ? failure.message : "The local view did not finish loading. No action has been automatically retried.");
    } finally { readInFlight = false; render(); schedule(busy() ? 750 : 4000); }
  }
  async function act(action) {
    if (!current?.view || busy() || (action !== "reload" && (needsReload() || current.view.fresh_execution !== "executed_this_process" || !current.view.available_actions.includes(action)))) return;
    const body = actionBody(action, current.view.revision);
    requestInFlight = true; activeAction = action; clearError(); render();
    if (["prepare_funding", "prepare_payment"].includes(action)) byId("terminal-note").focus();
    try {
      const accepted = await request("/api/action", body);
      decimal(accepted.job_id); pendingJobId = accepted.job_id;
    } catch (failure) {
      localNeedsReload = true;
      showError(failure?.kind || "action_inconclusive", failure?.message && failure.kind ? failure.message : "The local action response did not finish. It may still be running or saved. Reload checked state before another action; no automatic retry was sent.");
    } finally { requestInFlight = false; render(); schedule(100); }
  }
  if (!hash(capability)) {
    showError("session_unavailable", "This page has no valid local session capability. Reopen the URL printed by the current server.");
    set("connection-status", "Local session unavailable.");
    return;
  }
  buttons.forEach((button) => button.addEventListener("click", () => { if (ACTIONS.includes(button.dataset.action)) void act(button.dataset.action); }));
  document.addEventListener("visibilitychange", () => { if (!document.hidden) schedule(100); });
  void poll();
})();
