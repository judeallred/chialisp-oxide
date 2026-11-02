"use strict";
(() => {
  const capability = document.querySelector('meta[name="oxide-session"]').content;
  const buttons = [...document.querySelectorAll("[data-action]")];
  let snapshot = null, pending = false;
  const error = document.getElementById("error");
  const hash = bytes => bytes.map(v => v.toString(16).padStart(2, "0")).join("");
  async function request(path, body) {
    const response = await fetch(path, {method: body ? "POST" : "GET", credentials: "same-origin", headers: {"X-Oxide-Session": capability, ...(body ? {"Content-Type": "application/json"} : {})}, ...(body ? {body: JSON.stringify(body)} : {})});
    const text = await response.text();
    if (text.length > 65536) throw new Error("Local response too large");
    const value = JSON.parse(text);
    if (!response.ok) throw new Error(value.error?.message || "Local action failed; reload the journal");
    return value;
  }
  function render(value) {
    snapshot = value;
    const view = value.view;
    if (!view || view.protocol !== "chialisp-oxide-continuation-live/v1" || !/^[0-9a-f]{64}$/.test(view.revision)) throw new Error("Invalid local session view");
    document.getElementById("status").textContent = value.job ? `${value.job.status}: ${value.job.action}` : "Ready";
    if (value.job?.error) { error.hidden = false; error.textContent = value.job.error.message; }
    const rows = {"Wallet": view.wallet, "Rust mode": view.plan.mode, "Selected module": hash(view.plan.module_hash), "Funding amount (mojos)": view.plan.steps[0].amount, "Funding fee (mojos)": view.funding_fee, "Step sponsor fee (mojos)": view.step_fee, "Prepared transactions": view.prepared_transactions, "Observed state": view.current_state || "None", "Push started": String(view.push_started), "Journal revision": view.revision};
    const summary = document.getElementById("summary"); summary.replaceChildren();
    for (const [name, content] of Object.entries(rows)) { const box = document.createElement("div"), dt = document.createElement("dt"), dd = document.createElement("dd"); dt.textContent = name; dd.textContent = String(content); dd.className = "hash"; box.append(dt, dd); summary.append(box); }
    const queue = document.getElementById("queue"); queue.replaceChildren();
    view.plan.steps.forEach(step => { const li = document.createElement("li"); li.textContent = `Recipient ${hash(step.recipient)} · committed instance ${hash(step.puzzle_hash)}`; li.className = "hash"; queue.append(li); });
    document.getElementById("scope").textContent = view.scope;
    buttons.forEach(button => { button.disabled = pending || value.busy || (value.needs_reload && button.dataset.action !== "reload") || !view.available_actions.includes(button.dataset.action); });
  }
  async function reload() { try {render(await request("/api/state"));} catch (e) {error.hidden=false;error.textContent=e.message;} }
  buttons.forEach(button => button.addEventListener("click", async () => {
    if (pending || !snapshot) return;
    pending=true; buttons.forEach(b=>b.disabled=true); error.hidden=true;
    try { await request("/api/action", {action: button.dataset.action, revision: snapshot.view.revision}); }
    catch (e) {error.hidden=false;error.textContent=e.message;}
    finally {pending=false;await reload();}
  }));
  reload(); setInterval(reload, 1500);
})();
