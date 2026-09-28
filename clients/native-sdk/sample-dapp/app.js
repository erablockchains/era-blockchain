import { EraReadOnlyClient } from "../sdk/era-rpc-client.mjs";

const form = document.querySelector("#connect-form");
const endpoint = document.querySelector("#endpoint");
const status = document.querySelector("#status");
const snapshotView = document.querySelector("#snapshot");

function setField(name, value) {
  document.querySelector(`[data-field="${name}"]`).textContent = String(value ?? "unavailable");
}

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  snapshotView.hidden = true;
  status.textContent = "Reading node identity…";

  try {
    const client = new EraReadOnlyClient(endpoint.value);
    const snapshot = await client.getNodeSnapshot();

    setField("chain", snapshot.chain);
    setField("node", `${snapshot.nodeName} ${snapshot.nodeVersion}`);
    setField("block", Number.parseInt(snapshot.header.number, 16));
    setField("spec", snapshot.runtime.specVersion);
    setField("transaction", snapshot.runtime.transactionVersion);
    setField("peers", snapshot.health?.peers);
    setField("syncing", snapshot.health?.isSyncing);

    const expected = snapshot.runtime.specVersion === 14 && snapshot.runtime.transactionVersion === 1;
    status.textContent = expected
      ? "Expected V14 version numbers observed; hashes still require independent verification."
      : "Version mismatch. Stop review and verify the selected binary and chain spec.";
    snapshotView.hidden = false;
  } catch (error) {
    status.textContent = `Connection failed: ${error.message}`;
  }
});
