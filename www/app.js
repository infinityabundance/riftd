let wasmApi = null;

const inviteInput = document.getElementById("invite");
const relayInput = document.getElementById("relay");
const inviteStatus = document.getElementById("invite-status");
const transportStatus = document.getElementById("transport-status");
const chat = document.getElementById("chat");
const messageInput = document.getElementById("message");

const createBtn = document.getElementById("create-invite");
const joinBtn = document.getElementById("join-invite");
const connectBtn = document.getElementById("connect");
const disconnectBtn = document.getElementById("disconnect");
const sendBtn = document.getElementById("send");

let wasmReady = false;
let client = null;
let ws = null;
let room = null;
let peerId = null;

function setInviteStatus(text) {
  inviteStatus.textContent = text;
}

function setTransportStatus(text) {
  transportStatus.textContent = text;
}

function logMessage(text, from) {
  const entry = document.createElement("div");
  entry.className = "entry";
  entry.textContent = from ? `${from.slice(0, 8)}: ${text}` : text;
  chat.appendChild(entry);
  chat.scrollTop = chat.scrollHeight;
}

function bytesToBase64(bytes) {
  let binary = "";
  bytes.forEach((b) => (binary += String.fromCharCode(b)));
  return btoa(binary);
}

function base64ToBytes(base64) {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

async function ensureWasm() {
  if (wasmReady) {
    return;
  }
  if (!wasmApi) {
    try {
      wasmApi = await import("./pkg/rift_wasm.js");
    } catch (_) {
      wasmApi = await import("./rift_wasm.js");
    }
  }
  await wasmApi.default();
  wasmReady = true;
}

createBtn.addEventListener("click", async () => {
  await ensureWasm();
  const invite = wasmApi.create_invite("rift-demo", null);
  inviteInput.value = invite;
  setInviteStatus("Invite created.");
});

joinBtn.addEventListener("click", async () => {
  await ensureWasm();
  try {
    const invite = inviteInput.value.trim();
    if (!invite) {
      setInviteStatus("Paste an invite link first.");
      return;
    }
    const info = wasmApi.inspect_invite(invite);
    client = wasmApi.join_invite(invite);
    room = client.session_id;
    peerId = client.peer_id;
    setInviteStatus(`Joined ${info.channel_name} (${peerId.slice(0, 8)})`);
  } catch (err) {
    setInviteStatus(`Join failed: ${err}`);
  }
});

connectBtn.addEventListener("click", () => {
  if (!client) {
    setTransportStatus("Join an invite first.");
    return;
  }
  if (ws) {
    setTransportStatus("Already connected.");
    return;
  }
  const url = relayInput.value.trim();
  ws = new WebSocket(url);
  ws.addEventListener("open", () => {
    const join = {
      type: "join",
      room,
      peer_id: peerId,
    };
    ws.send(JSON.stringify(join));
    setTransportStatus(`Connected to ${url}`);
    logMessage("Connected to relay.");
  });
  ws.addEventListener("message", (event) => {
    try {
      const payload = JSON.parse(event.data);
      if (payload.type === "data" && payload.peer_id !== peerId) {
        const bytes = base64ToBytes(payload.data);
        const decoded = client.decode_text(bytes);
        logMessage(decoded.text, decoded.from);
      }
    } catch (err) {
      console.warn("bad message", err);
    }
  });
  ws.addEventListener("close", () => {
    ws = null;
    setTransportStatus("Disconnected");
    logMessage("Relay disconnected.");
  });
});

disconnectBtn.addEventListener("click", () => {
  if (ws) {
    ws.close();
  }
});

sendBtn.addEventListener("click", () => {
  if (!client || !ws) {
    setTransportStatus("Join + connect first.");
    return;
  }
  const text = messageInput.value.trim();
  if (!text) {
    return;
  }
  const bytes = client.encode_text(text);
  const payload = {
    type: "data",
    room,
    peer_id: peerId,
    data: bytesToBase64(bytes),
  };
  ws.send(JSON.stringify(payload));
  logMessage(text, "me");
  messageInput.value = "";
});
