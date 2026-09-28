import { encodeEraV14Call } from "../sdk/era-v14-native.mjs";
import { signEraV14CallWithInjectedProvider } from "../sdk/era-v14-injected-wallet.mjs";

export async function createOfflineWalletExample({ registry, metadataHex, account, genesisHash, blockHash }) {
  const callHex = encodeEraV14Call("EraWorlds", "registerWorld", {
    worldId: "offline-example",
    commitment: `0x${"55".repeat(32)}`,
  });
  return signEraV14CallWithInjectedProvider({
    source: "mock-era-wallet",
    applicationName: "ERA V14 offline integration example",
    address: account,
    callHex,
    runtimeVersion: { specVersion: 14, transactionVersion: 1 },
    metadataHex,
    genesisHash,
    expectedGenesisHash: genesisHash,
    blockHash,
    blockNumber: 42,
    nonce: 7,
    tip: 0,
    mortality: { period: 64 },
    registry,
  });
}
