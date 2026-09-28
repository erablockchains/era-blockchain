import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

const SAFE_CHAIN_TYPES = new Set(["development", "local"]);
const REVIEW_SYMBOLS_PENDING_INTEGRATION = new Set(["ERA", "ETKN"]);

export function validateChainSpec(spec) {
  const errors = [];
  if (!spec || typeof spec !== "object" || Array.isArray(spec)) {
    return ["chain spec must be a JSON object"];
  }

  const name = typeof spec.name === "string" ? spec.name.trim() : "";
  const id = typeof spec.id === "string" ? spec.id.trim() : "";
  if (!name || !id) errors.push("chain spec requires non-empty name and id");
  if (/\b(mainnet|production|prod|live)\b/i.test(`${name} ${id}`)) {
    errors.push("chain spec name/id must identify a disposable review network");
  }

  const chainType = typeof spec.chainType === "string" ? spec.chainType.toLowerCase() : "";
  if (!SAFE_CHAIN_TYPES.has(chainType)) {
    errors.push("chainType must be Development or Local");
  }
  if (!Array.isArray(spec.bootNodes) || spec.bootNodes.length !== 0) {
    errors.push("bootNodes must be an empty array");
  }
  if (!(spec.telemetryEndpoints === null ||
    (Array.isArray(spec.telemetryEndpoints) && spec.telemetryEndpoints.length === 0))) {
    errors.push("telemetryEndpoints must be null or empty");
  }
  if (!(spec.protocolId === null || spec.protocolId === undefined || spec.protocolId === "")) {
    errors.push("protocolId must be absent or empty for this isolated template");
  }

  if (!spec.genesis || typeof spec.genesis !== "object" || Array.isArray(spec.genesis)) {
    errors.push("chain spec requires a genesis object");
  }
  if (!spec.properties || typeof spec.properties !== "object" || Array.isArray(spec.properties)) {
    errors.push("chain spec requires properties");
  } else {
    if (spec.properties.tokenDecimals !== 18) {
      errors.push("tokenDecimals must be 18");
    }
    if (!REVIEW_SYMBOLS_PENDING_INTEGRATION.has(spec.properties.tokenSymbol)) {
      errors.push("tokenSymbol must match the checkpoint/policy review set pending integration");
    }
  }
  return errors;
}

export async function validateChainSpecFile(filePath) {
  const source = await readFile(filePath, "utf8");
  let spec;
  try {
    spec = JSON.parse(source);
  } catch (error) {
    throw new Error(`chain spec is not valid JSON: ${error.message}`);
  }
  const errors = validateChainSpec(spec);
  if (errors.length) {
    throw new Error(errors.join("; "));
  }
  return {
    sha256: createHash("sha256").update(source).digest("hex"),
    name: spec.name,
    id: spec.id,
    chainType: spec.chainType,
    tokenSymbol: spec.properties.tokenSymbol,
    tokenDecimals: spec.properties.tokenDecimals,
  };
}

const invokedPath = process.argv[1] ? pathToFileURL(process.argv[1]).href : "";
if (invokedPath === import.meta.url) {
  const filePath = process.argv[2];
  if (!filePath) {
    console.error("ERA_REVIEW_CHAIN_SPEC_STATUS=BLOCKED_MISSING_PATH");
    process.exitCode = 1;
  } else {
    try {
      const summary = await validateChainSpecFile(filePath);
      console.log("ERA_REVIEW_CHAIN_SPEC_STATUS=PASS_SANITIZED_DISPOSABLE_SPEC");
      console.log(JSON.stringify(summary));
    } catch (error) {
      console.error(`ERA_REVIEW_CHAIN_SPEC_STATUS=BLOCKED_${error.message}`);
      process.exitCode = 1;
    }
  }
}
