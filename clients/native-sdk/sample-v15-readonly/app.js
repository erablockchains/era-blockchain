import {EraReadOnlyClient} from '../sdk/era-rpc-client.mjs';
import {verifyEraSpec16FinalizedContext} from '../sdk/era-spec16-context.mjs';

const form = document.querySelector('#connect-form');
const result = document.querySelector('#result');
const status = document.querySelector('#status');

form.addEventListener('submit', async event => {
  event.preventDefault();
  result.hidden = true;
  status.textContent = 'Verifying finalized ERA identity…';
  try {
    const client = new EraReadOnlyClient(document.querySelector('#endpoint').value);
    const context = await verifyEraSpec16FinalizedContext(client);
    document.querySelector('#height').textContent = String(context.height);
    document.querySelector('#hash').textContent = context.at;
    document.querySelector('#spec').textContent = `${context.runtime.specName} ${context.runtime.specVersion}`;
    document.querySelector('#metadata').textContent = context.metadataSha256;
    result.hidden = false;
    status.textContent = 'Published spec16 identity verified at this finalized block.';
  } catch (error) {
    status.textContent = `Verification failed: ${error.message}`;
  }
});
