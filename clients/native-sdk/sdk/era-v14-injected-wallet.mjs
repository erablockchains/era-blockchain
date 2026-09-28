import {decodeEraAccount} from './era-account.mjs';
import {
  ERA_V14_NATIVE_BINDINGS,
  EraV14CodecError,
  assertEraV14ChainContext,
  buildEraV14SigningPayload,
  encodeEraMortality,
  encodeEraV14SignedExtrinsic,
  hexToBytes,
} from "./era-v14-native.mjs";

export class EraV14InjectedWalletError extends Error {
  constructor(message) {
    super(message);
    this.name = "EraV14InjectedWalletError";
  }
}

function fail(message) {
  throw new EraV14InjectedWalletError(message);
}

function signerHex(value, label, bytes) {
  if (typeof value === "boolean" || (typeof value === "number" && !Number.isSafeInteger(value))) throw new TypeError(`${label} must be an exact unsigned integer`);
  let integer;
  try {
    integer = typeof value === "bigint" ? value : BigInt(value);
  } catch {
    throw new TypeError(`${label} must be an unsigned integer`);
  }
  if (integer < 0n || integer >= (1n << BigInt(bytes * 8))) throw new RangeError(`${label} must fit its unsigned signer field`);
  return `0x${integer.toString(16).padStart(bytes * 2, "0")}`;
}

function normalizeTypedSignature(signature) {
  const bytes = hexToBytes(signature, "injected signature");
  if (bytes.length < 1 || bytes[0] > 2) fail("injected signer returned an unsupported MultiSignature type");
  const signatureTypes = ["Ed25519", "Sr25519", "Ecdsa"];
  const expectedRawLength = bytes[0] === 2 ? 65 : 64;
  const rawBytes = bytes.slice(1);
  if (rawBytes.length !== expectedRawLength) {
    fail(`injected signer returned an invalid ${signatureTypes[bytes[0]]} signature length`);
  }
  return {
    signatureType: signatureTypes[bytes[0]],
    signature: `0x${Array.from(rawBytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`,
  };
}

export function selectEraV14InjectedProvider(source, registry = globalThis.injectedWeb3) {
  if (typeof source !== "string" || source.length === 0) fail("wallet source is required");
  const provider = registry?.[source];
  if (!provider || typeof provider.enable !== "function") fail("requested injected wallet provider is unavailable");
  return provider;
}

export async function signEraV14CallWithInjectedProvider({
  bindings = ERA_V14_NATIVE_BINDINGS,
  injectedProvider,
  source,
  applicationName,
  address,
  accountId = address,
  callHex,
  runtimeVersion,
  metadataHex,
  genesisHash,
  expectedGenesisHash,
  blockHash,
  blockNumber,
  nonce,
  tip = 0,
  mortality,
  signedExtensions = bindings.signedExtensions,
  registry = globalThis.injectedWeb3,
}) {
  if (typeof applicationName !== "string" || applicationName.length === 0) fail("applicationName is required");
  if (typeof address !== "string" || address.length === 0) fail("address is required");
  const decodedAddress = decodeEraAccount(address);
  if (decodeEraAccount(accountId) !== decodedAddress) fail("signer address and AccountId disagree");
  accountId = decodedAddress;
  const era = mortality ? encodeEraMortality({ period: mortality.period, current: blockNumber }) : { type: "Immortal" };
  if (era.type === "Mortal" && !blockHash) fail("blockHash is required for a mortal transaction");
  await assertEraV14ChainContext({ bindings, runtimeVersion, metadataHex, genesisHash, expectedGenesisHash, signedExtensions });

  const provider = selectEraV14InjectedProvider(source, registry);
  const injected = injectedProvider ?? await provider.enable(applicationName);
  if (!injected?.accounts || typeof injected.accounts.get !== "function") fail("injected provider has no standard accounts.get interface");
  if (!injected?.signer || typeof injected.signer.signPayload !== "function") fail("injected provider has no standard signer.signPayload interface");
  const accounts = await injected.accounts.get();
  if (!Array.isArray(accounts) || !accounts.some((account) => account?.address === address)) {
    fail("selected address is not exposed by the injected provider");
  }

  const signing = buildEraV14SigningPayload({ bindings, callHex, nonce, tip, genesisHash, blockHash, era });
  const signerPayload = Object.freeze({
    address,
    blockHash: era.type === "Immortal" ? genesisHash : blockHash,
    blockNumber: signerHex(blockNumber, "blockNumber", 4),
    era: signing.era.encodedHex,
    genesisHash,
    method: callHex,
    nonce: signerHex(nonce, "nonce", 4),
    signedExtensions: [...bindings.signedExtensions],
    specVersion: signerHex(bindings.specVersion, "specVersion", 4),
    tip: signerHex(tip, "tip", 16),
    transactionVersion: signerHex(bindings.transactionVersion, "transactionVersion", 4),
    version: bindings.extrinsicVersion,
  });
  const result = await injected.signer.signPayload(signerPayload);
  if (!result || typeof result.signature !== "string") fail("injected signer returned no signature");
  const typed = normalizeTypedSignature(result.signature);
  const extrinsicHex = encodeEraV14SignedExtrinsic({
    callHex,
    signer: accountId,
    signatureType: typed.signatureType,
    signature: typed.signature,
    nonce,
    tip,
    era: signing.era,
  });
  return Object.freeze({
    providerSource: source,
    signerResultId: result.id,
    signerPayload,
    signingPayloadHex: signing.payloadHex,
    extrinsicHex,
    mortality: signing.era,
  });
}

export { EraV14CodecError };
