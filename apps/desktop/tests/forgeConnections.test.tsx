import assert from "node:assert/strict";
import test from "node:test";
import { renderToStaticMarkup } from "react-dom/server";

import { cancelForgeSignIn, connectForge, reloadForgeConnections } from "../src/app/forgeConnections";
import { GitHubAccessDialog } from "../src/components/forge/GitHubAccessDialog";

test("reconnecting keeps the old credential and displays the new device code", async () => {
  const runtime = globalThis as typeof globalThis & {
    window?: unknown;
    __TAURI_INTERNALS__?: unknown;
  };
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousInternals = Object.getOwnPropertyDescriptor(globalThis, "__TAURI_INTERNALS__");
  const commands: string[] = [];
  let releaseStart!: () => void;
  const startGate = new Promise<void>((resolve) => { releaseStart = resolve; });
  let started!: () => void;
  const didStart = new Promise<void>((resolve) => { started = resolve; });
  let pageOpened!: () => void;
  const didOpen = new Promise<void>((resolve) => { pageOpened = resolve; });
  let refreshed = false;
  let granted = true;
  Object.defineProperty(runtime, "window", { configurable: true, value: runtime });
  Object.defineProperty(runtime, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {
      invoke: async (command: string) => {
        commands.push(command);
        if (command === "forge_credentials") return [{
          host: "github.com", hint: refreshed ? "new" : "old", kind: "oauth", account: "owner",
          missing_scopes: refreshed && granted ? [] : ["workflow"],
        }];
        if (command === "forge_login_start") {
          started();
          await startGate;
          return {
            host: "github.com", user_code: "ABCD-EFGH", verification_uri: "https://github.com/login/device",
            interval_seconds: 1, expires_in_seconds: 60,
          };
        }
        if (command === "plugin:opener|open_url") { pageOpened(); return; }
        if (command === "forge_login_poll") {
          refreshed = true;
          return { state: "complete", account: { host: "github.com", login: "owner" } };
        }
        throw new Error(`Unexpected command: ${command}`);
      },
    },
  });
  let connecting: Promise<void> | undefined;
  try {
    await reloadForgeConnections();
    const render = () => renderToStaticMarkup(
      <GitHubAccessDialog onClose={() => undefined} />,
    );
    assert.match(render(), /Sign in again/);

    connecting = connectForge("github.com");
    await didStart;
    await connectForge("github.com");
    assert.equal(commands.filter((command) => command === "forge_login_start").length, 1);
    assert.match(render(), /Requesting a sign-in code/);

    releaseStart();
    await didOpen;
    const pending = render();
    assert.match(pending, /ABCD-EFGH/);
    assert.match(pending, /Copy the sign-in code/);
    assert.match(pending, /Cancel/);
    assert.doesNotMatch(pending, /Disconnect from/);
    // Rendering a second time models reopening the dialog while polling runs.
    assert.match(render(), /ABCD-EFGH/);
    await connecting;
    assert.equal(render(), "", "successful sign-in dismisses the recovery dialog");
    // Completing the device flow alone does not prove all scopes were granted.
    granted = false;
    await reloadForgeConnections();
    assert.match(render(), /Restore GitHub access/);
    assert.match(render(), /before GitCat asked for the workflow permission/);
    assert.ok(!commands.includes("forge_sign_out"));
    assert.ok(!commands.includes("forge_token_set"));
  } finally {
    releaseStart();
    cancelForgeSignIn();
    await connecting;
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else Reflect.deleteProperty(globalThis, "window");
    if (previousInternals) Object.defineProperty(globalThis, "__TAURI_INTERNALS__", previousInternals);
    else Reflect.deleteProperty(globalThis, "__TAURI_INTERNALS__");
  }
});
