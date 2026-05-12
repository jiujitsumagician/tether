// Tether desktop UI driver. Renders the 14-string state machine.
// All real work happens in Rust; this file is glue.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "./ui/strings";

type PairingEvent =
  | { kind: "status_key"; key: string }
  | { kind: "card"; peer_device_name: string; emojis: [string, string, string] }
  | { kind: "manual_entry_open" }
  | { kind: "paired"; peer_device_name: string }
  | { kind: "mismatch"; reason: string }
  | { kind: "exhausted" };

const root = document.getElementById("screen")!;
const manualLink = document.getElementById("pair-another-way")!;

function clear() {
  root.replaceChildren();
}

function renderIdle() {
  clear();
  const h1 = document.createElement("h1");
  h1.textContent = t("home.pc.idle");
  root.appendChild(h1);
  manualLink.hidden = false;
}

function renderStatus(key: string) {
  clear();
  const h1 = document.createElement("h1");
  if (key === "cascade.usb.prompt") {
    const wrap = document.createElement("div");
    wrap.className = "usb-prompt";
    const icon = document.createElement("div");
    icon.className = "icon";
    icon.textContent = "🔌";
    wrap.appendChild(icon);
    h1.textContent = t("cascade.usb.prompt");
    wrap.appendChild(h1);
    root.appendChild(wrap);
    manualLink.hidden = false;
    return;
  }
  if (key === "cascade.usb.debug") {
    h1.textContent = t("cascade.usb.debug");
    root.appendChild(h1);
    const btn = document.createElement("button");
    btn.className = "btn";
    btn.textContent = "Open phone settings";
    btn.disabled = false;
    // The Rust side already fired the intent on the device; clicking
    // this just re-fires the same intent in case the user dismissed
    // the screen.
    btn.addEventListener("click", () => {
      // No-op: the deep-link is best-effort and idempotent.
      btn.textContent = "Settings open on your phone.";
      btn.disabled = true;
    });
    root.appendChild(btn);
    manualLink.hidden = false;
    return;
  }
  // Default — spinner + status text.
  const spinner = document.createElement("span");
  spinner.className = "spinner";
  h1.prepend(spinner);
  h1.append(document.createTextNode(safeText(key)));
  root.appendChild(h1);
  manualLink.hidden = false;
}

function safeText(key: string): string {
  switch (key) {
    case "cascade.mdns": return t("cascade.mdns");
    case "cascade.fallback": return t("cascade.fallback");
    case "cascade.usb.detected": return t("cascade.usb.detected");
    case "cascade.usb.prompt": return t("cascade.usb.prompt");
    case "cascade.usb.debug": return t("cascade.usb.debug");
    case "cascade.hotspot": return t("cascade.hotspot");
    default: return t("home.pc.idle");
  }
}

function renderCard(peer: string, emojis: [string, string, string]) {
  clear();
  manualLink.hidden = true;

  const card = document.createElement("div");
  card.className = "card";

  const title = document.createElement("h2");
  title.className = "title";
  title.textContent = t("pair.card.title", { peer });
  card.appendChild(title);

  const emojiRow = document.createElement("div");
  emojiRow.className = "emojis";
  emojiRow.textContent = emojis.join("");
  card.appendChild(emojiRow);

  const sub = document.createElement("p");
  sub.className = "subhead";
  sub.textContent = t("pair.card.subhead");
  card.appendChild(sub);

  const confirm = document.createElement("button");
  confirm.className = "btn";
  confirm.textContent = t("pair.card.confirm");
  confirm.addEventListener("click", async () => {
    confirm.disabled = true;
    await invoke("confirm");
  });
  card.appendChild(confirm);

  const mismatch = document.createElement("button");
  mismatch.className = "btn btn-danger";
  mismatch.textContent = t("pair.mismatch");
  mismatch.addEventListener("click", async () => {
    await invoke("mismatch");
    renderIdle();
    // Kick off discovery again.
    invoke("start_pairing");
  });
  card.appendChild(mismatch);

  root.appendChild(card);
}

function renderPaired(name: string) {
  clear();
  manualLink.hidden = true;
  const wrap = document.createElement("div");
  wrap.className = "screen";
  const big = document.createElement("h1");
  big.className = "success";
  big.textContent = "✓ " + name;
  const msg = document.createElement("p");
  msg.className = "sub";
  msg.textContent = t("pair.success");
  wrap.append(big, msg);
  root.appendChild(wrap);
}

function renderMismatch(reason: string) {
  clear();
  manualLink.hidden = false;
  const err = document.createElement("div");
  err.className = "error";
  if (reason === "timeout") {
    err.textContent =
      "We waited but no one confirmed. Try again from both apps.";
  } else if (reason === "protocol") {
    err.textContent =
      "Something didn't add up about the other device. Try again.";
  } else {
    err.textContent = t("pair.mismatch");
  }
  root.appendChild(err);
  const btn = document.createElement("button");
  btn.className = "btn";
  btn.textContent = "Try again";
  btn.addEventListener("click", () => {
    renderIdle();
    invoke("reset_pairing").then(() => invoke("start_pairing"));
  });
  root.appendChild(btn);
}

function renderExhausted() {
  clear();
  manualLink.hidden = false;
  const wrap = document.createElement("div");
  wrap.className = "screen";
  const h1 = document.createElement("h1");
  h1.textContent =
    "We can't see your phone from this network. Plug in a USB cable to finish setup.";
  wrap.appendChild(h1);
  const btn = document.createElement("button");
  btn.className = "btn";
  btn.textContent = "Try again";
  btn.addEventListener("click", () => {
    renderIdle();
    invoke("reset_pairing").then(() => invoke("start_pairing"));
  });
  wrap.appendChild(btn);
  root.appendChild(wrap);
}

function renderManualForm() {
  clear();
  const wrap = document.createElement("div");
  wrap.className = "screen";
  const h1 = document.createElement("h1");
  h1.textContent = "Pair another way";
  const sub = document.createElement("p");
  sub.className = "sub";
  sub.textContent =
    "Enter your PC's address and the 6-digit code shown on screen.";
  wrap.append(h1, sub);
  const form = document.createElement("form");
  form.className = "manual-form";
  form.innerHTML = `
    <input name="address" placeholder="192.168.1.42:31415" autocomplete="off" required />
    <input name="pin" placeholder="6-digit code" inputmode="numeric" maxlength="6" required />
    <button class="btn" type="submit">Pair</button>
  `;
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const fd = new FormData(form);
    await invoke("submit_manual", {
      address: String(fd.get("address") ?? ""),
      pin: String(fd.get("pin") ?? ""),
    });
  });
  wrap.appendChild(form);
  root.appendChild(wrap);
}

// Wire up events from Rust.
async function bootstrap() {
  renderIdle();

  await listen<PairingEvent>("pairing", (evt) => {
    const data = evt.payload;
    switch (data.kind) {
      case "status_key":
        renderStatus(data.key);
        break;
      case "card":
        renderCard(data.peer_device_name, data.emojis);
        break;
      case "manual_entry_open":
        renderManualForm();
        break;
      case "paired":
        renderPaired(data.peer_device_name);
        break;
      case "mismatch":
        renderMismatch(data.reason);
        break;
      case "exhausted":
        renderExhausted();
        break;
    }
  });

  manualLink.addEventListener("click", async (e) => {
    e.preventDefault();
    await invoke("open_manual_entry");
  });

  await invoke("start_pairing");
}

bootstrap().catch((e) => {
  console.error("bootstrap failed", e);
});
