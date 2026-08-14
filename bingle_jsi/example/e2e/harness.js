/**
 * Shared helpers for driving the bingle_jsi command-dispatcher harness from Detox (issue #109).
 *
 * The harness (App.tsx) exposes: a JSON command box (`cmd-input`), a Run button (`cmd-run`), a
 * status line (`cmd-status`: idle|running|ok|error), a JSON result (`cmd-output`), and an event
 * feed (`event-feed`). These helpers wrap "type a command, run it, read the result" so tests read
 * as a sequence of API calls.
 */

/** How long to wait for a command's status to settle. */
const COMMAND_TIMEOUT = 30000;

/** Type a `{ method, args }` command into the box and run it. */
async function runCommand(command) {
  await element(by.id('cmd-input')).replaceText(JSON.stringify(command));
  await element(by.id('cmd-run')).tap();
}

/** Wait until the status line reaches `expected` ('ok' or 'error'). */
async function expectStatus(expected) {
  await waitFor(element(by.id('cmd-status')))
    .toHaveText(expected)
    .withTimeout(COMMAND_TIMEOUT);
}

/** Run a command, assert it succeeded, and return its parsed JSON result. */
async function call(command) {
  await runCommand(command);
  await expectStatus('ok');
  const attrs = await element(by.id('cmd-output')).getAttributes();
  const text = attrs.text ?? attrs.label ?? attrs.value ?? '';
  return text === '' ? null : JSON.parse(text);
}

/** Run a command expected to fail, and return the error text shown in the output. */
async function callExpectingError(command) {
  await runCommand(command);
  await expectStatus('error');
  const attrs = await element(by.id('cmd-output')).getAttributes();
  return attrs.text ?? attrs.label ?? attrs.value ?? '';
}

module.exports = {runCommand, expectStatus, call, callExpectingError};
