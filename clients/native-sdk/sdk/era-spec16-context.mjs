// Read-only V15 SDK prerequisite. These identities come from the published
// native-ETKN integration kit v1.0.0, not from an unverified RPC response.
export const ERA_SPEC16 = Object.freeze({
  genesis: '0x0abc2c3d8db5815541050b73da4d81267ebf14d90dbee8d7258155b667ea112e',
  specName: 'era',
  specVersion: 16,
  transactionVersion: 1,
  stateVersion: 1,
  metadataSha256: '574931f693241cb62e1ad2990eda55e2250c36c9c0de852b0380eac480c8496a',
});

function metadataBytes(hex) {
  if (typeof hex !== 'string' || !/^0x(?:[0-9a-fA-F]{2})+$/.test(hex) || hex.length > 2_000_002) {
    throw new Error('invalid or oversized finalized metadata');
  }
  return Uint8Array.from(hex.slice(2).match(/../g), byte => Number.parseInt(byte, 16));
}

export async function verifyEraSpec16FinalizedContext(client) {
  const genesis = await client.getGenesisHash();
  if (genesis.toLowerCase() !== ERA_SPEC16.genesis) throw new Error('ERA genesis mismatch');

  const at = await client.getFinalizedHead();
  const [header, runtime, metadataHex] = await Promise.all([
    client.getHeader(at),
    client.getRuntimeVersion(at),
    client.getMetadataHex(at),
  ]);
  if (runtime.specName !== ERA_SPEC16.specName ||
      runtime.specVersion !== ERA_SPEC16.specVersion ||
      runtime.transactionVersion !== ERA_SPEC16.transactionVersion ||
      runtime.stateVersion !== ERA_SPEC16.stateVersion) {
    throw new Error('ERA spec16 runtime identity mismatch');
  }
  const digest = await globalThis.crypto.subtle.digest('SHA-256', metadataBytes(metadataHex));
  const metadataSha256 = Array.from(new Uint8Array(digest), b => b.toString(16).padStart(2, '0')).join('');
  if (metadataSha256 !== ERA_SPEC16.metadataSha256) throw new Error('ERA spec16 metadata mismatch');
  const height = Number.parseInt(header.number, 16);
  if (!Number.isSafeInteger(height) || height < 0) throw new Error('invalid finalized height');
  return Object.freeze({ at, height, genesis, runtime: Object.freeze({...runtime}), metadataSha256 });
}
