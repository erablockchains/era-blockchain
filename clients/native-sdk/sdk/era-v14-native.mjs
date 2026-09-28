const textEncoder = new TextEncoder();

const PALLETS = Object.freeze({
  Balances: Object.freeze({
    index: 2,
    calls: Object.freeze({ transferAllowDeath: 0, transferKeepAlive: 3 }),
  }),
  Assets: Object.freeze({
    index: 16,
    calls: Object.freeze({
      create: 0,
      mint: 6,
      burn: 7,
      transfer: 8,
      transferKeepAlive: 9,
      setMetadata: 17,
      clearMetadata: 18,
      approveTransfer: 22,
      cancelApproval: 23,
      transferApproved: 25,
    }),
  }),
  Nfts: Object.freeze({
    index: 17,
    calls: Object.freeze({
      createCollection: 0,
      mint: 3,
      burn: 5,
      transfer: 6,
      lockItemTransfer: 8,
      unlockItemTransfer: 9,
      setMetadata: 24,
      startCollectionRetirement: 39,
      continueCollectionRetirement: 40,
      continueDelegateCleanup: 41,
    }),
  }),
  EraWorlds: Object.freeze({
    index: 18,
    calls: Object.freeze({ registerWorld: 0, updateCommitment: 1, deregisterWorld: 2 }),
  }),
});

const EVENTS = Object.freeze({
  Balances: Object.freeze({ 2: "Transfer" }),
  Assets: Object.freeze({ 2: "Transferred" }),
  Nfts: Object.freeze({
    38: "CollectionRetirementStarted",
    39: "CollectionRetirementProgress",
    40: "AttributeCountReconciled",
    41: "DelegateCleanupProgress",
  }),
  EraWorlds: Object.freeze({
    0: "WorldRegistered",
    1: "CommitmentUpdated",
    2: "WorldRemoved",
    3: "Paused",
    4: "Unpaused",
  }),
});

const ERRORS = Object.freeze({
  Balances: Object.freeze({ 0: "VestingBalance", 2: "InsufficientBalance" }),
  Assets: Object.freeze({ 2: "NoPermission", 3: "Unknown", 9: "BadMetadata" }),
  Nfts: Object.freeze({
    0: "NoPermission",
    1: "UnknownCollection",
    44: "WitnessRequired",
    45: "CleanupLimitExceeded",
    46: "CollectionRetiring",
    47: "NotRetiring",
    48: "DelegateCleanupPending",
    49: "CleanupStateInvalid",
    50: "RefundFailed",
  }),
  EraWorlds: Object.freeze({
    0: "RegistryPaused",
    1: "AlreadyRegistered",
    2: "UnknownWorld",
    3: "NotOwner",
    4: "DepositInvariant",
    5: "AlreadyPaused",
    6: "NotPaused",
  }),
});

export const ERA_V14_NATIVE_BINDINGS = Object.freeze({
  historicalFreezeGateBaselineCommit: "e450a4dbcb2850733bdc24ad7d2e551f9671015c",
  historicalFreezeGateBaselineTree: "5ff62958df789811de6c1a05a108a48ceec65e56",
  historicalBindingSourceCommit: "cc37097ca93211a4f9c042569f610ad735d0d30a",
  historicalBindingSourceTree: "9944372a78df3f9d62c7f2ee71a7a0f2c989856e",
  deployedSourceCandidateCommit: "5ff1f84f84e6e1b809420c9899f5aceeb3f8b5be",
  genesisHash: "0x0abc2c3d8db5815541050b73da4d81267ebf14d90dbee8d7258155b667ea112e",
  metadataEncoding: "raw RuntimeMetadataPrefixed (state_getMetadata)",
  metadataSha256: "eed011f659bd492aedb643d775fce7cab26adb89c46db8598f4b2c079897e5d2",
  specVersion: 14,
  transactionVersion: 1,
  extrinsicVersion: 4,
  addressType: "MultiAddress::Id(AccountId32)",
  signedExtensions: Object.freeze([
    "CheckNonZeroSender",
    "CheckSpecVersion",
    "CheckTxVersion",
    "CheckGenesis",
    "CheckMortality",
    "CheckNonce",
    "CheckWeight",
    "ChargeTransactionPayment",
  ]),
  limits: Object.freeze({
    assetNameBytes: 64,
    assetSymbolBytes: 64,
    nftMetadataBytes: 128,
    worldIdBytes: 64,
    nftCleanupItemsPerCall: 685,
  }),
  pallets: PALLETS,
  events: EVENTS,
  errors: ERRORS,
  availability: Object.freeze({
    nativeEtknTransfers: "REACHABLE",
    registeredAssets: "REACHABLE",
    nfts: "REACHABLE",
    nftRetirement: "REACHABLE",
    eraWorlds: "REACHABLE",
    versionedAssetRuntimeApi: "DORMANT_SOURCE_ONLY",
    amm: "DORMANT_SOURCE_ONLY",
    contracts: "ABSENT",
    evm: "ABSENT",
    erc20: "NOT_IMPLEMENTED",
  }),
});

export class EraV14CodecError extends Error {
  constructor(message) {
    super(message);
    this.name = "EraV14CodecError";
  }
}

function fail(message) {
  throw new EraV14CodecError(message);
}

function concatBytes(...parts) {
  const length = parts.reduce((total, part) => total + part.length, 0);
  const output = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.length;
  }
  return output;
}

export function bytesToHex(bytes) {
  if (!(bytes instanceof Uint8Array)) {
    throw new TypeError("bytes must be a Uint8Array");
  }
  return `0x${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

export function hexToBytes(hex, label = "hex") {
  if (typeof hex !== "string" || !/^0x(?:[0-9a-fA-F]{2})*$/.test(hex)) {
    throw new TypeError(`${label} must be even-length 0x-prefixed bytes`);
  }
  const output = new Uint8Array((hex.length - 2) / 2);
  for (let index = 0; index < output.length; index += 1) {
    output[index] = Number.parseInt(hex.slice(2 + index * 2, 4 + index * 2), 16);
  }
  return output;
}

function requireBytes(value, label, expectedLength) {
  const bytes = value instanceof Uint8Array ? value : hexToBytes(value, label);
  if (expectedLength !== undefined && bytes.length !== expectedLength) {
    throw new TypeError(`${label} must contain exactly ${expectedLength} bytes`);
  }
  return bytes;
}

function requireUnsigned(value, label, bits) {
  if (typeof value === "boolean" || (typeof value === "number" && !Number.isSafeInteger(value))) throw new TypeError(`${label} must be an exact unsigned integer; use a string or bigint for large values`);
  let integer;
  try {
    integer = typeof value === "bigint" ? value : BigInt(value);
  } catch {
    throw new TypeError(`${label} must be an unsigned integer`);
  }
  const maximum = (1n << BigInt(bits)) - 1n;
  if (integer < 0n || integer > maximum) {
    throw new RangeError(`${label} must fit in u${bits}`);
  }
  return integer;
}

function encodeUnsigned(value, byteLength, label) {
  let integer = requireUnsigned(value, label, byteLength * 8);
  const output = new Uint8Array(byteLength);
  for (let index = 0; index < byteLength; index += 1) {
    output[index] = Number(integer & 0xffn);
    integer >>= 8n;
  }
  return output;
}

export function encodeScaleCompact(value, label = "compact value") {
  const integer = requireUnsigned(value, label, 536);
  if (integer < 1n << 6n) {
    return Uint8Array.of(Number(integer << 2n));
  }
  if (integer < 1n << 14n) {
    return encodeUnsigned((integer << 2n) | 1n, 2, label);
  }
  if (integer < 1n << 30n) {
    return encodeUnsigned((integer << 2n) | 2n, 4, label);
  }

  const encoded = [];
  let remaining = integer;
  while (remaining > 0n) {
    encoded.push(Number(remaining & 0xffn));
    remaining >>= 8n;
  }
  while (encoded.length < 4) encoded.push(0);
  if (encoded.length > 67) fail(`${label} is too large for SCALE compact encoding`);
  return Uint8Array.of(((encoded.length - 4) << 2) | 3, ...encoded);
}

function encodeVector(value, label, maximumLength) {
  const bytes = typeof value === "string" ? textEncoder.encode(value) : requireBytes(value, label);
  if (bytes.length > maximumLength) {
    throw new RangeError(`${label} exceeds the ${maximumLength}-byte V14 bound`);
  }
  return concatBytes(encodeScaleCompact(bytes.length, `${label} length`), bytes);
}

function encodeAccountId(value, label = "account") {
  return requireBytes(value, label, 32);
}

function encodeMultiAddressId(value, label = "account") {
  return concatBytes(Uint8Array.of(0), encodeAccountId(value, label));
}

function encodeCallPrefix(pallet, method) {
  const palletBinding = PALLETS[pallet];
  if (!palletBinding || !Object.hasOwn(palletBinding.calls,method)) {
    fail(`unsupported ERA V14 call ${pallet}.${method}`);
  }
  return Uint8Array.of(palletBinding.index, palletBinding.calls[method]);
}

function encodeDefaultNftCollectionConfig() {
  return concatBytes(
    encodeUnsigned(0, 8, "collection settings"),
    Uint8Array.of(0),
    Uint8Array.of(0, 0, 0, 0),
    encodeUnsigned(0, 8, "item settings"),
  );
}

function encodeBalancesCall(method, args) {
  const prefix = encodeCallPrefix("Balances", method);
  if (method !== "transferAllowDeath" && method !== "transferKeepAlive") {
    fail(`unsupported ERA V14 Balances call ${method}`);
  }
  return concatBytes(
    prefix,
    encodeMultiAddressId(args.dest, "dest"),
    encodeScaleCompact(requireUnsigned(args.amount, "amount", 128), "amount"),
  );
}

function encodeAssetsCall(method, args) {
  const prefix = encodeCallPrefix("Assets", method);
  const id = encodeUnsigned(args.id, 4, "asset id");
  switch (method) {
    case "create":
      return concatBytes(
        prefix,
        id,
        encodeMultiAddressId(args.admin, "admin"),
        encodeUnsigned(args.minBalance, 16, "minBalance"),
      );
    case "mint":
      return concatBytes(
        prefix,
        id,
        encodeMultiAddressId(args.beneficiary, "beneficiary"),
        encodeScaleCompact(requireUnsigned(args.amount, "amount", 128), "amount"),
      );
    case "burn":
      return concatBytes(
        prefix,
        id,
        encodeMultiAddressId(args.who, "who"),
        encodeScaleCompact(requireUnsigned(args.amount, "amount", 128), "amount"),
      );
    case "transfer":
    case "transferKeepAlive":
      return concatBytes(
        prefix,
        id,
        encodeMultiAddressId(args.target, "target"),
        encodeScaleCompact(requireUnsigned(args.amount, "amount", 128), "amount"),
      );
    case "setMetadata": {
      const decimals = requireUnsigned(args.decimals, "decimals", 8);
      return concatBytes(
        prefix,
        id,
        encodeVector(args.name, "name", ERA_V14_NATIVE_BINDINGS.limits.assetNameBytes),
        encodeVector(args.symbol, "symbol", ERA_V14_NATIVE_BINDINGS.limits.assetSymbolBytes),
        Uint8Array.of(Number(decimals)),
      );
    }
    case "clearMetadata":
      return concatBytes(prefix, id);
    case "approveTransfer":
      return concatBytes(
        prefix,
        id,
        encodeMultiAddressId(args.delegate, "delegate"),
        encodeScaleCompact(requireUnsigned(args.amount, "amount", 128), "amount"),
      );
    case "cancelApproval":
      return concatBytes(prefix, id, encodeMultiAddressId(args.delegate, "delegate"));
    case "transferApproved":
      return concatBytes(
        prefix,
        id,
        encodeMultiAddressId(args.owner, "owner"),
        encodeMultiAddressId(args.destination, "destination"),
        encodeScaleCompact(requireUnsigned(args.amount, "amount", 128), "amount"),
      );
    default:
      fail(`unsupported ERA V14 Assets call ${method}`);
  }
}

function encodeNftsCall(method, args) {
  const prefix = encodeCallPrefix("Nfts", method);
  switch (method) {
    case "createCollection":
      return concatBytes(
        prefix,
        encodeMultiAddressId(args.admin, "admin"),
        encodeDefaultNftCollectionConfig(),
      );
    case "mint": {
      const collection = encodeUnsigned(args.collection, 4, "collection");
      return concatBytes(
        prefix,
        collection,
        encodeUnsigned(args.item, 4, "item"),
        encodeMultiAddressId(args.mintTo, "mintTo"),
        Uint8Array.of(0),
      );
    }
    case "burn": {
      const collection = encodeUnsigned(args.collection, 4, "collection");
      return concatBytes(prefix, collection, encodeUnsigned(args.item, 4, "item"));
    }
    case "transfer": {
      const collection = encodeUnsigned(args.collection, 4, "collection");
      return concatBytes(
        prefix,
        collection,
        encodeUnsigned(args.item, 4, "item"),
        encodeMultiAddressId(args.dest, "dest"),
      );
    }
    case "lockItemTransfer":
    case "unlockItemTransfer": {
      const collection = encodeUnsigned(args.collection, 4, "collection");
      return concatBytes(prefix, collection, encodeUnsigned(args.item, 4, "item"));
    }
    case "setMetadata": {
      const collection = encodeUnsigned(args.collection, 4, "collection");
      return concatBytes(
        prefix,
        collection,
        encodeUnsigned(args.item, 4, "item"),
        encodeVector(args.data, "data", ERA_V14_NATIVE_BINDINGS.limits.nftMetadataBytes),
      );
    }
    case "startCollectionRetirement": {
      const collection = encodeUnsigned(args.collection, 4, "collection");
      return concatBytes(prefix, collection);
    }
    case "continueCollectionRetirement": {
      const collection = encodeUnsigned(args.collection, 4, "collection");
      const limit = requireUnsigned(args.limit, "limit", 32);
      if (limit === 0n || limit > BigInt(ERA_V14_NATIVE_BINDINGS.limits.nftCleanupItemsPerCall)) {
        throw new RangeError("limit must be between 1 and the V14 NFT cleanup ceiling");
      }
      return concatBytes(prefix, collection, encodeUnsigned(limit, 4, "limit"));
    }
    case "continueDelegateCleanup": {
      const collection = encodeUnsigned(args.collection, 4, "collection");
      const limit = requireUnsigned(args.limit, "limit", 32);
      if (limit === 0n || limit > BigInt(ERA_V14_NATIVE_BINDINGS.limits.nftCleanupItemsPerCall)) {
        throw new RangeError("limit must be between 1 and the V14 NFT cleanup ceiling");
      }
      return concatBytes(
        prefix,
        collection,
        encodeUnsigned(args.item, 4, "item"),
        encodeAccountId(args.delegate, "delegate"),
        encodeUnsigned(limit, 4, "limit"),
      );
    }
    default:
      fail(`unsupported ERA V14 Nfts call ${method}`);
  }
}

function encodeWorldsCall(method, args) {
  const prefix = encodeCallPrefix("EraWorlds", method);
  const worldId = encodeVector(args.worldId, "worldId", ERA_V14_NATIVE_BINDINGS.limits.worldIdBytes);
  switch (method) {
    case "registerWorld":
    case "updateCommitment":
      return concatBytes(prefix, worldId, requireBytes(args.commitment, "commitment", 32));
    case "deregisterWorld":
      return concatBytes(prefix, worldId);
    default:
      fail(`unsupported ERA V14 EraWorlds call ${method}`);
  }
}

export function encodeEraV14Call(pallet, method, args = {}) {
  let encoded;
  switch (pallet) {
    case "Balances":
      encoded = encodeBalancesCall(method, args);
      break;
    case "Assets":
      encoded = encodeAssetsCall(method, args);
      break;
    case "Nfts":
      encoded = encodeNftsCall(method, args);
      break;
    case "EraWorlds":
      encoded = encodeWorldsCall(method, args);
      break;
    default:
      fail(`unsupported ERA V14 pallet ${pallet}`);
  }
  return bytesToHex(encoded);
}

class ScaleReader {
  constructor(input) {
    this.bytes = requireBytes(input, "encoded bytes");
    this.offset = 0;
  }

  take(length, label) {
    if (!Number.isSafeInteger(length) || length < 0 || this.offset + length > this.bytes.length) {
      fail(`truncated ${label}`);
    }
    const value = this.bytes.slice(this.offset, this.offset + length);
    this.offset += length;
    return value;
  }

  u8(label) {
    return this.take(1, label)[0];
  }

  unsigned(byteLength, label) {
    const bytes = this.take(byteLength, label);
    let value = 0n;
    for (let index = bytes.length - 1; index >= 0; index -= 1) {
      value = (value << 8n) | BigInt(bytes[index]);
    }
    return value;
  }

  compact(label) {
    const first = this.u8(label);
    const mode = first & 3;
    if (mode === 0) return BigInt(first >> 2);
    if (mode === 1) {
      const second = this.u8(label);
      return BigInt((first | (second << 8)) >> 2);
    }
    if (mode === 2) {
      const rest = this.take(3, label);
      const encoded = BigInt(first) | (BigInt(rest[0]) << 8n) | (BigInt(rest[1]) << 16n) | (BigInt(rest[2]) << 24n);
      return encoded >> 2n;
    }
    const byteLength = (first >> 2) + 4;
    return new ScaleReader(this.take(byteLength, label)).unsigned(byteLength, label);
  }

  vector(label, maximumLength) {
    const length = this.compact(`${label} length`);
    if (length > BigInt(maximumLength) || length > BigInt(Number.MAX_SAFE_INTEGER)) {
      fail(`${label} exceeds its V14 bound`);
    }
    return this.take(Number(length), label);
  }

  accountId(label) {
    return bytesToHex(this.take(32, label));
  }

  multiAddressId(label) {
    if (this.u8(`${label} address type`) !== 0) fail(`${label} must use MultiAddress::Id`);
    return this.accountId(label);
  }

  rest() {
    return this.take(this.bytes.length - this.offset, "remaining bytes");
  }

  done(label = "encoded value") {
    if (this.offset !== this.bytes.length) fail(`${label} contains trailing bytes`);
  }
}

function numberOrBigInt(value) {
  return value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value;
}

function decodeCallReader(reader) {
  const palletIndex = reader.u8("pallet index");
  const callIndex = reader.u8("call index");
  const entry = Object.entries(PALLETS).find(([, binding]) => binding.index === palletIndex);
  if (!entry) fail(`unsupported ERA V14 pallet index ${palletIndex}`);
  const [pallet, binding] = entry;
  const method = Object.entries(binding.calls).find(([, index]) => index === callIndex)?.[0];
  if (!method) fail(`unsupported ERA V14 call index ${palletIndex}:${callIndex}`);

  const args = {};
  if (pallet === "Balances") {
    args.dest = reader.multiAddressId("dest");
    args.amount = numberOrBigInt(reader.compact("amount"));
  } else if (pallet === "Assets") {
    args.id = Number(reader.unsigned(4, "asset id"));
    if (method === "create") {
      args.admin = reader.multiAddressId("admin");
      args.minBalance = numberOrBigInt(reader.unsigned(16, "minBalance"));
    } else if (method === "mint") {
      args.beneficiary = reader.multiAddressId("beneficiary");
      args.amount = numberOrBigInt(reader.compact("amount"));
    } else if (method === "burn") {
      args.who = reader.multiAddressId("who");
      args.amount = numberOrBigInt(reader.compact("amount"));
    } else if (method === "transfer" || method === "transferKeepAlive") {
      args.target = reader.multiAddressId("target");
      args.amount = numberOrBigInt(reader.compact("amount"));
    } else if (method === "setMetadata") {
      args.nameHex = bytesToHex(reader.vector("name", ERA_V14_NATIVE_BINDINGS.limits.assetNameBytes));
      args.symbolHex = bytesToHex(reader.vector("symbol", ERA_V14_NATIVE_BINDINGS.limits.assetSymbolBytes));
      args.decimals = reader.u8("decimals");
    } else if (method === "approveTransfer") {
      args.delegate = reader.multiAddressId("delegate");
      args.amount = numberOrBigInt(reader.compact("amount"));
    } else if (method === "cancelApproval") {
      args.delegate = reader.multiAddressId("delegate");
    } else if (method === "transferApproved") {
      args.owner = reader.multiAddressId("owner");
      args.destination = reader.multiAddressId("destination");
      args.amount = numberOrBigInt(reader.compact("amount"));
    }
  } else if (pallet === "Nfts") {
    if (method === "createCollection") {
      args.admin = reader.multiAddressId("admin");
      args.configHex = bytesToHex(reader.take(21, "default collection config"));
    } else {
      args.collection = Number(reader.unsigned(4, "collection"));
      if (method === "mint") {
        args.item = Number(reader.unsigned(4, "item"));
        args.mintTo = reader.multiAddressId("mintTo");
        if (reader.u8("mint witness") !== 0) fail("only the no-witness mint binding is supported");
      } else if (["burn", "lockItemTransfer", "unlockItemTransfer"].includes(method)) {
        args.item = Number(reader.unsigned(4, "item"));
      } else if (method === "transfer") {
        args.item = Number(reader.unsigned(4, "item"));
        args.dest = reader.multiAddressId("dest");
      } else if (method === "setMetadata") {
        args.item = Number(reader.unsigned(4, "item"));
        args.dataHex = bytesToHex(reader.vector("data", ERA_V14_NATIVE_BINDINGS.limits.nftMetadataBytes));
      } else if (method === "continueCollectionRetirement") {
        args.limit = Number(reader.unsigned(4, "limit"));
      } else if (method === "continueDelegateCleanup") {
        args.item = Number(reader.unsigned(4, "item"));
        args.delegate = reader.accountId("delegate");
        args.limit = Number(reader.unsigned(4, "limit"));
      }
    }
  } else if (pallet === "EraWorlds") {
    args.worldIdHex = bytesToHex(reader.vector("worldId", ERA_V14_NATIVE_BINDINGS.limits.worldIdBytes));
    if (method === "registerWorld" || method === "updateCommitment") {
      args.commitment = bytesToHex(reader.take(32, "commitment"));
    }
  }

  return { palletIndex, callIndex, pallet, method, args };
}

export function decodeEraV14Call(input) {
  const reader = new ScaleReader(input);
  const call = decodeCallReader(reader);
  reader.done("call");
  return call;
}

function requirePowerOfTwoPeriod(period) {
  const value = requireUnsigned(period, "mortality period", 32);
  if (value < 4n || value > 65_536n || (value & (value - 1n)) !== 0n) {
    throw new RangeError("mortality period must be a power of two from 4 through 65536");
  }
  return value;
}

export function encodeEraMortality({ period, current }) {
  const normalizedPeriod = requirePowerOfTwoPeriod(period);
  const currentBlock = requireUnsigned(current, "current block", 64);
  const quantizeFactor = normalizedPeriod > 4_096n ? normalizedPeriod >> 12n : 1n;
  const phase = currentBlock % normalizedPeriod;
  const quantizedPhase = (phase / quantizeFactor) * quantizeFactor;
  let trailing = 0n;
  for (let value = normalizedPeriod; (value & 1n) === 0n; value >>= 1n) trailing += 1n;
  const encoded = Number((trailing - 1n) | ((quantizedPhase / quantizeFactor) << 4n));
  return Object.freeze({
    type: "Mortal",
    period: Number(normalizedPeriod),
    phase: Number(quantizedPhase),
    encodedHex: bytesToHex(encodeUnsigned(encoded, 2, "mortal era")),
  });
}

function normalizeEra(era = Object.freeze({ type: "Immortal" })) {
  if (era === "Immortal" || era?.type === "Immortal") {
    return Object.freeze({ type: "Immortal", encodedHex: "0x00" });
  }
  if (era?.type === "Mortal" && era.encodedHex !== undefined) {
    requireBytes(era.encodedHex, "mortal era", 2);
    return Object.freeze({ ...era, encodedHex: bytesToHex(requireBytes(era.encodedHex, "mortal era", 2)) });
  }
  if (era && era.period !== undefined && era.current !== undefined) return encodeEraMortality(era);
  fail("era must be Immortal or a caller-selected mortal period/current block");
}

export function buildEraV14SigningPayload({
  bindings = ERA_V14_NATIVE_BINDINGS,
  callHex,
  nonce,
  tip = 0,
  genesisHash,
  blockHash,
  era = Object.freeze({ type: "Immortal" }),
}) {
  const call = requireBytes(callHex, "callHex");
  const genesis = requireBytes(genesisHash, "genesisHash", 32);
  const normalizedEra = normalizeEra(era);
  const mortalityHash = normalizedEra.type === "Immortal"
    ? genesis
    : requireBytes(blockHash, "blockHash", 32);
  const payload = concatBytes(
    call,
    requireBytes(normalizedEra.encodedHex, "era"),
    encodeScaleCompact(requireUnsigned(nonce, "nonce", 32), "nonce"),
    encodeScaleCompact(requireUnsigned(tip, "tip", 128), "tip"),
    encodeUnsigned(bindings.specVersion, 4, "specVersion"),
    encodeUnsigned(bindings.transactionVersion, 4, "transactionVersion"),
    genesis,
    mortalityHash,
  );
  return Object.freeze({
    payloadHex: bytesToHex(payload),
    payloadByteLength: payload.length,
    requiresBlake2_256: payload.length > 256,
    era: normalizedEra,
  });
}

const SIGNATURES = Object.freeze({
  Ed25519: Object.freeze({ index: 0, bytes: 64 }),
  Sr25519: Object.freeze({ index: 1, bytes: 64 }),
  Ecdsa: Object.freeze({ index: 2, bytes: 65 }),
});

export function encodeEraV14SignedExtrinsic({
  callHex,
  signer,
  signatureType,
  signature,
  nonce,
  tip = 0,
  era = Object.freeze({ type: "Immortal" }),
}) {
  const signatureBinding = SIGNATURES[signatureType];
  if (!signatureBinding) fail(`unsupported signature type ${signatureType}`);
  const body = concatBytes(
    Uint8Array.of(0x80 | ERA_V14_NATIVE_BINDINGS.extrinsicVersion),
    encodeMultiAddressId(signer, "signer"),
    Uint8Array.of(signatureBinding.index),
    requireBytes(signature, "signature", signatureBinding.bytes),
    requireBytes(normalizeEra(era).encodedHex, "era"),
    encodeScaleCompact(requireUnsigned(nonce, "nonce", 32), "nonce"),
    encodeScaleCompact(requireUnsigned(tip, "tip", 128), "tip"),
    requireBytes(callHex, "callHex"),
  );
  return bytesToHex(concatBytes(encodeScaleCompact(body.length, "extrinsic length"), body));
}

export function decodeEraV14Extrinsic(input) {
  const reader = new ScaleReader(input);
  const bodyLength = reader.compact("extrinsic length");
  if (bodyLength !== BigInt(reader.bytes.length - reader.offset)) fail("extrinsic length prefix mismatch");
  const versionByte = reader.u8("extrinsic version");
  const signed = (versionByte & 0x80) !== 0;
  const version = versionByte & 0x7f;
  if (version !== ERA_V14_NATIVE_BINDINGS.extrinsicVersion) {
    fail(`unsupported extrinsic version ${version}`);
  }
  if (!signed) {
    const call = decodeCallReader(reader);
    reader.done("extrinsic");
    return { signed, version, call };
  }

  const signer = reader.multiAddressId("signer");
  const signatureIndex = reader.u8("signature type");
  const signatureEntry = Object.entries(SIGNATURES).find(([, binding]) => binding.index === signatureIndex);
  if (!signatureEntry) fail(`unsupported signature variant ${signatureIndex}`);
  const [signatureType, signatureBinding] = signatureEntry;
  const signature = bytesToHex(reader.take(signatureBinding.bytes, "signature"));
  const eraFirst = reader.u8("era");
  const era = eraFirst === 0
    ? Object.freeze({ type: "Immortal" })
    : Object.freeze({ type: "Mortal", encodedHex: bytesToHex(Uint8Array.of(eraFirst, reader.u8("mortal era"))) });
  const nonce = numberOrBigInt(reader.compact("nonce"));
  const tip = numberOrBigInt(reader.compact("tip"));
  const call = decodeCallReader(reader);
  reader.done("extrinsic");
  return { signed, version, signer, signatureType, signature, era, nonce, tip, call };
}

export function assertEraV14RuntimeVersion(runtimeVersion, bindings = ERA_V14_NATIVE_BINDINGS) {
  if (
    !runtimeVersion
    || runtimeVersion.specVersion !== bindings.specVersion
    || runtimeVersion.transactionVersion !== bindings.transactionVersion
  ) {
    fail("runtime spec/transaction version does not match the ERA V14 native bindings");
  }
  return true;
}

export async function assertEraV14Metadata(metadataHex, bindings = ERA_V14_NATIVE_BINDINGS) {
  if (!globalThis.crypto?.subtle) fail("WebCrypto SHA-256 is required for metadata verification");
  const digest = new Uint8Array(await globalThis.crypto.subtle.digest("SHA-256", hexToBytes(metadataHex, "metadataHex")));
  if (bytesToHex(digest).slice(2) !== bindings.metadataSha256) {
    fail("runtime metadata hash does not match the ERA V14 native bindings");
  }
  return true;
}

export async function assertEraV14ChainContext({
  bindings = ERA_V14_NATIVE_BINDINGS,
  runtimeVersion,
  metadataHex,
  genesisHash,
  expectedGenesisHash = ERA_V14_NATIVE_BINDINGS.genesisHash,
  signedExtensions,
}) {
  assertEraV14RuntimeVersion(runtimeVersion, bindings);
  await assertEraV14Metadata(metadataHex, bindings);
  const actualGenesis = bytesToHex(requireBytes(genesisHash, "genesisHash", 32));
  const expectedGenesis = bytesToHex(requireBytes(expectedGenesisHash, "expectedGenesisHash", 32));
  if (actualGenesis !== expectedGenesis) fail("genesis hash does not match the caller-pinned chain");
  if (
    !Array.isArray(signedExtensions)
    || signedExtensions.length !== ERA_V14_NATIVE_BINDINGS.signedExtensions.length
    || signedExtensions.some((extension, index) => extension !== ERA_V14_NATIVE_BINDINGS.signedExtensions[index])
  ) {
    fail("signed extensions or ordering do not match the ERA V14 metadata binding");
  }
  return true;
}

export async function detectEraV14Transition({ previousRuntimeVersion, runtimeVersion, metadataHex }) {
  if (previousRuntimeVersion?.specVersion !== 13 || previousRuntimeVersion?.transactionVersion !== 1) {
    fail("previous runtime is not the anchored ERA spec-13 transition source");
  }
  assertEraV14RuntimeVersion(runtimeVersion);
  await assertEraV14Metadata(metadataHex);
  return Object.freeze({
    transition: "ERA_SPEC_13_TO_14",
    metadataRefreshed: true,
    previousSpecVersion: 13,
    specVersion: 14,
    transactionVersion: 1,
  });
}

function eventBinding(palletIndex, eventIndex) {
  const entry = Object.entries(PALLETS).find(([, binding]) => binding.index === palletIndex);
  if (!entry) fail(`unsupported ERA V14 event pallet index ${palletIndex}`);
  const [pallet] = entry;
  const event = EVENTS[pallet]?.[eventIndex];
  if (!event) fail(`unsupported ERA V14 event index ${palletIndex}:${eventIndex}`);
  return { pallet, event };
}

export function decodeEraV14Event(input) {
  const reader = new ScaleReader(input);
  const palletIndex = reader.u8("event pallet index");
  const eventIndex = reader.u8("event index");
  const { pallet, event } = eventBinding(palletIndex, eventIndex);
  const fields = {};
  if (pallet === "Balances" && event === "Transfer") {
    fields.from = reader.accountId("from");
    fields.to = reader.accountId("to");
    fields.amount = numberOrBigInt(reader.unsigned(16, "amount"));
  } else if (pallet === "Assets" && event === "Transferred") {
    fields.assetId = Number(reader.unsigned(4, "asset id"));
    fields.from = reader.accountId("from");
    fields.to = reader.accountId("to");
    fields.amount = numberOrBigInt(reader.unsigned(16, "amount"));
  } else if (pallet === "Nfts") {
    fields.collection = Number(reader.unsigned(4, "collection"));
    if (event === "CollectionRetirementProgress") {
      fields.removed = Number(reader.unsigned(4, "removed"));
      fields.phase = reader.u8("phase");
    } else if (event === "AttributeCountReconciled") {
      fields.discrepancy = Number(reader.unsigned(4, "discrepancy"));
    } else if (event === "DelegateCleanupProgress") {
      fields.item = Number(reader.unsigned(4, "item"));
      fields.delegate = reader.accountId("delegate");
      fields.removed = Number(reader.unsigned(4, "removed"));
      const complete = reader.u8("complete");
      if (complete > 1) fail("invalid bool in DelegateCleanupProgress");
      fields.complete = complete === 1;
    }
  } else {
    fields.encodedHex = bytesToHex(reader.rest());
  }
  reader.done("event");
  return { palletIndex, eventIndex, pallet, event, fields };
}

export function decodeEraV14ModuleError({ palletIndex, errorHex }) {
  const error = requireBytes(errorHex, "module error", 4);
  if (error[1] !== 0 || error[2] !== 0 || error[3] !== 0) fail("module error has unsupported trailing bytes");
  const entry = Object.entries(PALLETS).find(([, binding]) => binding.index === palletIndex);
  if (!entry) fail(`unsupported ERA V14 error pallet index ${palletIndex}`);
  const [pallet] = entry;
  const name = ERRORS[pallet]?.[error[0]];
  if (!name) fail(`unsupported ERA V14 error index ${palletIndex}:${error[0]}`);
  return { palletIndex, errorIndex: error[0], pallet, error: name };
}

export function assertEraCallSupportedByMetadata(input, metadataBinding) {
  const bytes = requireBytes(input, "call");
  if (bytes.length < 2) fail("truncated call prefix");
  const palletIndex = bytes[0];
  const callIndex = bytes[1];
  const pallet = metadataBinding?.pallets?.find((entry) => entry.index === palletIndex);
  if (!pallet) fail(`pallet index ${palletIndex} is absent from supplied metadata`);
  const call = pallet.calls?.find((entry) => entry.index === callIndex);
  if (!call) fail(`call index ${palletIndex}:${callIndex} is absent from supplied metadata`);
  return Object.freeze({ pallet: pallet.name, call: call.name, palletIndex, callIndex });
}
