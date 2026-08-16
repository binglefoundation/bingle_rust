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

const fs = require('fs');
const {execSync} = require('child_process');

const COMMAND_TIMEOUT = 30000;
const POLL_INTERVAL = 400;

const sleep = ms => new Promise(r => setTimeout(r, ms));

// Android package id of the example app (see example/android/app/build.gradle).
const ANDROID_PKG = 'com.binglejsiexample';

/** First running emulator serial, so adb targets the right device. */
function androidSerial() {
  const out = execSync('adb devices').toString();
  const line = out.split('\n').find(l => /^emulator-\d+\s+device\b/.test(l));
  return line ? line.split(/\s+/)[0] : null;
}

/**
 * Resolve the network `init` inputs for the current platform (issue #131). The Detox test runs on
 * the host, but `node_file` is read by the app on the device — and unlike the iOS simulator (which
 * shares the host filesystem), the Android emulator has its own filesystem. So:
 *   - STUN is always passed inline via `stun_servers` (the host STUN file's contents;
 *     `parse_stun_list` accepts `#` comments and newlines), so no STUN file is needed on-device.
 *   - `node_file` is the host path on iOS; on Android the file's contents are written into the
 *     app's *internal* files dir via `run-as` (the debug app is debuggable) and that on-device path
 *     is returned. Internal storage is chosen because the app cannot read adb-pushed files under
 *     /sdcard/Android/data (scoped storage) or /data/local/tmp (SELinux) — both give EACCES.
 * Returns `{ node_file, stun_servers }` to spread into the init config (omit stun_servers_file).
 */
async function resolveNetworkInputs(hostNodeFile, hostStunFile) {
  const stun_servers = fs.readFileSync(hostStunFile, 'utf8');
  if (device.getPlatform() !== 'android') {
    return {node_file: hostNodeFile, stun_servers};
  }
  const serial = androidSerial();
  const adb = serial ? `adb -s ${serial}` : 'adb';
  const name = 'bingle_e2e_node.json';
  // base64 over `adb shell` avoids stdin/newline translation; toybox `base64 -d` decodes on-device.
  // run-as writes as the app uid, so the app can read the file back from its internal files dir.
  const b64 = fs.readFileSync(hostNodeFile).toString('base64');
  execSync(
    `${adb} shell "run-as ${ANDROID_PKG} sh -c 'mkdir -p files && echo ${b64} | base64 -d > files/${name}'"`,
  );
  return {node_file: `/data/data/${ANDROID_PKG}/files/${name}`, stun_servers};
}

/**
 * Platform-appropriate path for the local-API state file passed as `init`'s `local` (issue #131).
 * The app writes/reads this itself: on iOS the simulator shares the host filesystem so `/tmp` works;
 * Android has no `/tmp`, so use the app's internal files dir (always app-writable).
 */
function localStatePath(basename) {
  if (device.getPlatform() !== 'android') {
    return `/tmp/${basename}`;
  }
  return `/data/data/${ANDROID_PKG}/files/${basename}`;
}

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
  resolveNetworkInputs,
  localStatePath,
};
