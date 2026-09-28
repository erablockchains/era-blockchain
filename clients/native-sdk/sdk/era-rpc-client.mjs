export const DEFAULT_REVIEW_ENDPOINT = "http://127.0.0.1:9944";

export const READ_ONLY_RPC_METHODS = Object.freeze([
  "system_chain",
  "system_name",
  "system_version",
  "system_health",
  "chain_getHeader",
  "chain_getBlockHash",
  "chain_getBlock",
  "state_getRuntimeVersion",
  "state_getMetadata",
  "state_getStorage",
  "system_accountNextIndex",
  "payment_queryInfo",
]);

export class EraRpcError extends Error {
  constructor(message, details = {}) {
    super(message);
    this.name = "EraRpcError";
    this.code = details.code;
    this.data = details.data;
    this.method = details.method;
  }
}

function isLoopback(hostname) {
  return hostname === "127.0.0.1" || hostname === "localhost" || hostname === "[::1]";
}

function parseEndpoint(endpoint, allowRemote) {
  let url;
  try {
    url = new URL(endpoint);
  } catch {
    throw new TypeError("endpoint must be an absolute HTTP(S) URL");
  }

  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new TypeError("endpoint protocol must be HTTP or HTTPS");
  }
  if (url.username || url.password) {
    throw new TypeError("endpoint credentials are not accepted");
  }
  if (!isLoopback(url.hostname) && !allowRemote) {
    throw new TypeError("non-loopback endpoints require allowRemote: true");
  }
  if (!isLoopback(url.hostname) && url.protocol !== "https:") {
    throw new TypeError("non-loopback endpoints must use HTTPS");
  }
  return url.toString();
}

function requireAccount(account) {
  if (typeof account !== "string" || account.trim().length < 3) {
    throw new TypeError("account must be a non-empty encoded account string");
  }
  return account.trim();
}

function requireExtrinsicHex(extrinsicHex) {
  if (typeof extrinsicHex !== "string" || !/^0x(?:[0-9a-fA-F]{2})+$/.test(extrinsicHex)) {
    throw new TypeError("extrinsicHex must be non-empty, even-length 0x-prefixed bytes");
  }
  return extrinsicHex;
}

function requireHash(hash, label = "hash") {
  if (typeof hash !== "string" || !/^0x[0-9a-fA-F]{64}$/.test(hash)) {
    throw new TypeError(`${label} must be a 32-byte 0x-prefixed hash`);
  }
  return hash;
}

function requireStorageKey(storageKey) {
  if (typeof storageKey !== "string" || !/^0x(?:[0-9a-fA-F]{2})+$/.test(storageKey)) {
    throw new TypeError("storageKey must be non-empty, even-length 0x-prefixed bytes");
  }
  return storageKey;
}

export class EraReadOnlyClient {
  #endpoint;
  #fetch;
  #nextId = 1;
  #timeoutMs;

  constructor(endpoint = DEFAULT_REVIEW_ENDPOINT, options = {}) {
    const { allowRemote = false, fetchImpl = globalThis.fetch, timeoutMs = 10_000 } = options;
    if (typeof fetchImpl !== "function") {
      throw new TypeError("a fetch implementation is required");
    }
    if (!Number.isSafeInteger(timeoutMs) || timeoutMs <= 0) {
      throw new TypeError("timeoutMs must be a positive safe integer");
    }

    this.#endpoint = parseEndpoint(endpoint, allowRemote);
    this.#fetch = fetchImpl;
    this.#timeoutMs = timeoutMs;
  }

  get endpoint() {
    return this.#endpoint;
  }

  async #request(method, params = []) {
    if (!READ_ONLY_RPC_METHODS.includes(method)) {
      throw new EraRpcError("RPC method is outside the read-only allowlist", { method });
    }

    const id = this.#nextId++;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.#timeoutMs);
    let response;
    try {
      response = await this.#fetch(this.#endpoint, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id, method, params }),
        signal: controller.signal,
      });
    } catch (error) {
      const message = error?.name === "AbortError" ? "RPC request timed out" : "RPC transport failed";
      throw new EraRpcError(message, { method, data: error?.message });
    } finally {
      clearTimeout(timer);
    }

    if (!response || response.ok !== true) {
      throw new EraRpcError(`RPC HTTP status ${response?.status ?? "unknown"}`, { method });
    }

    let payload;
    try {
      payload = await response.json();
    } catch {
      throw new EraRpcError("RPC response was not valid JSON", { method });
    }

    if (!payload || payload.jsonrpc !== "2.0" || payload.id !== id) {
      throw new EraRpcError("RPC response envelope did not match the request", { method });
    }
    if (payload.error) {
      throw new EraRpcError(payload.error.message || "RPC returned an error", {
        method,
        code: payload.error.code,
        data: payload.error.data,
      });
    }
    if (!("result" in payload)) {
      throw new EraRpcError("RPC response omitted result", { method });
    }
    return payload.result;
  }

  async getNodeSnapshot() {
    const [chain, nodeName, nodeVersion, health, header, runtime] = await Promise.all([
      this.#request("system_chain"),
      this.#request("system_name"),
      this.#request("system_version"),
      this.#request("system_health"),
      this.#request("chain_getHeader"),
      this.#request("state_getRuntimeVersion"),
    ]);

    if (typeof chain !== "string" || typeof nodeName !== "string" || typeof nodeVersion !== "string") {
      throw new EraRpcError("node identity response had an unexpected shape");
    }
    if (!runtime || !Number.isSafeInteger(runtime.specVersion)) {
      throw new EraRpcError("runtime version response had an unexpected shape");
    }
    if (!header || typeof header.number !== "string") {
      throw new EraRpcError("chain header response had an unexpected shape");
    }

    return Object.freeze({ chain, nodeName, nodeVersion, health, header, runtime });
  }

  async getAccountNextIndex(account) {
    return this.#request("system_accountNextIndex", [requireAccount(account)]);
  }

  async getGenesisHash() {
    const hash = await this.#request("chain_getBlockHash", [0]);
    return requireHash(hash, "genesis hash");
  }

  async getMetadataHex(at = undefined) {
    const params = at === undefined ? [] : [requireHash(at, "at")];
    const metadata = await this.#request("state_getMetadata", params);
    if (typeof metadata !== "string" || !/^0x(?:[0-9a-fA-F]{2})+$/.test(metadata)) {
      throw new EraRpcError("runtime metadata response had an unexpected shape", {
        method: "state_getMetadata",
      });
    }
    return metadata;
  }

  async getStorage(storageKey, at = undefined) {
    const params = [requireStorageKey(storageKey)];
    if (at !== undefined) params.push(requireHash(at, "at"));
    const value = await this.#request("state_getStorage", params);
    if (value !== null && (typeof value !== "string" || !/^0x(?:[0-9a-fA-F]{2})*$/.test(value))) {
      throw new EraRpcError("storage response had an unexpected shape", { method: "state_getStorage" });
    }
    return value;
  }

  async getBlock(at = undefined) {
    const params = at === undefined ? [] : [requireHash(at, "at")];
    const signedBlock = await this.#request("chain_getBlock", params);
    if (!signedBlock?.block || !Array.isArray(signedBlock.block.extrinsics)) {
      throw new EraRpcError("block response had an unexpected shape", { method: "chain_getBlock" });
    }
    if (!signedBlock.block.extrinsics.every((value) => /^0x(?:[0-9a-fA-F]{2})+$/.test(value))) {
      throw new EraRpcError("block extrinsics had an unexpected shape", { method: "chain_getBlock" });
    }
    return signedBlock;
  }

  async queryFeeInfo(extrinsicHex, at = undefined) {
    const params = [requireExtrinsicHex(extrinsicHex)];
    if (at !== undefined) {
      params.push(requireHash(at, "at"));
    }
    return this.#request("payment_queryInfo", params);
  }
}
