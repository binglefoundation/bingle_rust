/**
 * Shared helpers for driving the bingle_jsi command-dispatcher harness from Detox (issue #109).
 *
 * The harness (App.tsx) exposes: a JSON command box (`cmd-input`), a Run button (`cmd-run`), a
 * status line (`cmd-status`: idle|running|ok|error), a JSON result (`cmd-output`), and an event
 * feed (`event-feed`). These helpers wrap "type a command, run it, read the result".
 *
 * Detox synchronization is disabled for this app (it runs a P2P engine with perpetual background
 * work, so it never reaches the fully-idle state Detox waits for). We therefore drive timing
 * explicitly: read element text with `getAttributes()` and poll, rather than Detox's
 * `waitFor().toHaveText()` (which is unreliable with synchronization disabled).
 */

const COMMAND_TIMEOUT = 30000;
const POLL_INTERVAL = 400;

const sleep = ms => new Promise(r => setTimeout(r, ms));

/** Read an element's visible text (iOS exposes it as text/label/value depending on the element). */
async function textOf(testID) {
  const attrs = await element(by.id(testID)).getAttributes();
  return attrs.text ?? attrs.label ?? attrs.value ?? '';
}

/** Type a `{ method, args }` command into the box and run it. */
async function runCommand(command) {
  await element(by.id('cmd-input')).replaceText(JSON.stringify(command));
  await element(by.id('cmd-run')).tap();
}

/**
 * Poll the status line until it reaches a terminal state. Returns the terminal status ('ok' or
 * 'error'). Throws on timeout.
 */
async function waitForTerminalStatus(timeoutMs = COMMAND_TIMEOUT) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const status = await textOf('cmd-status');
    if (status === 'ok' || status === 'error') {
      return status;
    }
    await sleep(POLL_INTERVAL);
  }
  throw new Error(`timed out after ${timeoutMs}ms waiting for a terminal status`);
}

/** Assert the last command reached the given status. */
async function expectStatus(expected) {
  const status = await waitForTerminalStatus();
  if (status !== expected) {
    const output = await textOf('cmd-output');
    throw new Error(`expected status "${expected}" but got "${status}": ${output}`);
  }
}

/** Run a command, assert it succeeded, and return its parsed JSON result. */
async function call(command) {
  await runCommand(command);
  const status = await waitForTerminalStatus();
  const output = await textOf('cmd-output');
  if (status !== 'ok') {
    throw new Error(`command ${JSON.stringify(command)} failed: ${output}`);
  }
  return output === '' ? null : JSON.parse(output);
}

/** Run a command expected to fail, and return the error text shown in the output. */
async function callExpectingError(command) {
  await runCommand(command);
  const status = await waitForTerminalStatus();
  const output = await textOf('cmd-output');
  if (status !== 'error') {
    throw new Error(`expected ${JSON.stringify(command)} to fail but it succeeded: ${output}`);
  }
  return output;
}

module.exports = {
  runCommand,
  expectStatus,
  call,
  callExpectingError,
  textOf,
  sleep,
};
