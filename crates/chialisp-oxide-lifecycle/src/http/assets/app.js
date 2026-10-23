"use strict";
(() => {
  const capability = document.querySelector('meta[name="oxide-session"]').content;
  const byId = (id) => document.getElementById(id);
  const stages = ["awaiting_build", "awaiting_check", "awaiting_funding", "awaiting_signature", "awaiting_signature_validation", "ready_to_submit", "awaiting_confirmation", "confirmed"];
  const labels = ["Import retained artifact", "Record executed checks", "Create simulator coin", "Prepare permissionless bundle", "Validate bundle", "Submit to simulator", "Read confirmed children", "Exercise complete"];
  let current = null;
  let busy = false;
  let needsReload = true;
  const hex = (bytes) => bytes.map((byte) => byte.toString(16).padStart(2, "0")).join("");

  function showError(kind, message) {
    const element = byId("error");
    element.hidden = false;
    element.textContent = `${kind}: ${message}`;
  }
  function render() {
    const view = current && current.view;
    const at = view ? stages.indexOf(view.stage) : -1;
    byId("advance").disabled = busy || needsReload || at < 0 || at === 7;
    const fresh = view && view.checks.fresh_cargo_build === "executed_this_process";
    byId("advance").textContent = busy ? "Saving trusted simulator action…" : (at === 0 && fresh ? "Import freshly checked instance" : (labels[at] || "Reload saved state"));
    byId("reload").disabled = busy;
    byId("observe").disabled = busy || needsReload || at < 3;
    byId("reorg").disabled = busy || needsReload || ![6, 7].includes(at);
    if (!view) return;
    byId("revision").textContent = view.revision;
    byId("stage").textContent = view.stage === "awaiting_signature"
      ? "awaiting bundle preparation" : view.stage.replaceAll("_", " ");
    byId("module-hash").textContent = view.module_tree_hash;
    byId("commit").textContent = fresh ? view.recipe_scope.native_input_fingerprint : view.retained_commit;
    byId("source-label").textContent = fresh ? "Current native input fingerprint" : "Pinned source commit";
    byId("artifact-badge").textContent = fresh ? `Fresh · ${view.recipe_scope.mode}` : "Retained";
    byId("artifact-origin").textContent = fresh ? "Built and checked in this process before the session opened. Finite consistency checks, not authentication." : "Imported artifact, not a fresh compilation.";
    byId("instance-hash").textContent = view.instance_tree_hash;
    byId("recipient-hash").textContent = view.destination_hex;
    byId("artifact-bytes").textContent = `${view.metrics.reveal_bytes} / ${view.metrics.solution_bytes} bytes`;
    const sdk = view.metrics.preflight_sdk_validation;
    byId("sdk-cost").textContent = `${sdk.cost.toLocaleString()} · flags ${sdk.flags} · ceiling ${sdk.max_cost.toLocaleString()}`;
    byId("plan-fee").textContent = `${view.metrics.plan_fee} mojos · plan value`;
    byId("cost-scope").textContent = `Executed preflight SDK validation: ${sdk.consensus} / ${sdk.sdk_vm}, ${sdk.network}, height ${sdk.height}. Deterministic preflight input, not the funded coin. Cost includes charged generator bytes, VM and conditions; not primary VM cost, full-block cost or a live fee quote.`;
    byId("recipe-description").textContent = fresh ? "Fresh Cargo/native, both Rust modes, independent Chialisp, VM and SDK checks ran before this session. Follow the selected curried instance in the simulator; no wallet is needed." : "Import a retained program, check its behavior, then follow a simulator coin through its spend and child output.";
    byId("import-label").textContent = fresh ? "Import freshly checked instance" : "Import retained artifact";
    byId("import-description").textContent = fresh ? "Exact selected Rust mode and public recipient committed before simulator funding." : "Optimized Rust CLVM from a pinned repository commit.";
    for (const id of ["fresh-cargo", "fresh-subset", "fresh-reference"]) {
      byId(id).textContent = fresh ? "Executed this process" : "Unavailable";
      byId(id).className = fresh ? "badge executed" : "badge neutral";
    }
    document.querySelectorAll("#steps li").forEach((step) => {
      const index = Number(step.dataset.at);
      step.classList.toggle("done", at > index);
      step.classList.toggle("active", at === index);
      step.querySelector(".step-state").textContent = at > index ? "Done" : (at === index ? "Next" : "");
    });
    if (view.input_coin) {
      byId("input-amount").textContent = `${view.input_coin.coin.amount.toLocaleString()} mojos`;
      byId("input-id").textContent = view.input_coin.coin_id;
    } else {
      byId("input-amount").textContent = "Not created yet";
      byId("input-id").textContent = "The simulator will assign an exact coin ID.";
    }
    const children = view.children || [];
    const child = children[0];
    byId("child-amount").textContent = child ? `${child.coin.amount.toLocaleString()} mojos · ${children.length} child` : "Waiting for a spend";
    byId("child-id").textContent = child ? child.coin_id : "A complete matching child set confirms this exercise.";
    byId("child-destination").hidden = !child;
    if (child) byId("child-destination").textContent = `Destination ${hex(child.coin.puzzle_hash)}`;
    const control = view.typed_controls && view.typed_controls[0];
    if (control) byId("typed-control").textContent = `${control.code} — underfunded submission rejected; full simulator state unchanged.`;
    byId("activity").textContent = needsReload ? "Reload durable state before trying another action." : (at === 7 ? "Exact input and child output observed in the SDK simulator." : "Saved state is current. Each action uses the shared trusted runner.");
  }

  async function request(path, body) {
    const headers = { "X-Oxide-Session": capability };
    if (body) headers["Content-Type"] = "application/json";
    const response = await fetch(path, {
      method: body ? "POST" : "GET", headers,
      body: body ? JSON.stringify(body) : undefined,
      mode: "same-origin", credentials: "omit", cache: "no-store", signal: AbortSignal.timeout(6000),
    });
    const value = await response.json();
    if (!response.ok) throw value.error || { kind: "http_failure", message: "The local server could not complete this request." };
    return value;
  }

  async function perform(command) {
    if (busy || (command && needsReload)) return;
    busy = true;
    render();
    try {
      current = await request(command ? "/api/commands" : "/api/state", command ? { revision: current.view.revision, command } : null);
      needsReload = false;
      byId("error").hidden = !current.last_error;
      if (current.last_error) showError(current.last_error.kind, current.last_error.message);
    } catch (error) {
      // A timeout does not cancel started filesystem work. Require a durable
      // state reload before retrying; every command also checks its revision.
      needsReload = true;
      showError(error.kind || "connection_failure", error.message || "Reload durable state before retrying. The server operation may still finish.");
    } finally {
      busy = false;
      render();
    }
  }
  byId("advance").addEventListener("click", () => perform("advance"));
  byId("observe").addEventListener("click", () => perform("refresh"));
  byId("reorg").addEventListener("click", () => perform("reorg_unspent"));
  byId("reload").addEventListener("click", () => perform(null));
  perform(null);
})();
