// A tiny MCP client over the stdio of `herbarium mcp`: one long-lived process,
// JSON-RPC requests, and `call(area.verb, args)` for the vault core. Used by
// the demo and screenshot tools to seed a vault the way an agent would.
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

/**
 * Start the MCP server on `vault` with `bin` (the CLI or the desktop binary).
 * `env` is merged over the current process, as the app would see it.
 */
export function mcpClient(bin, vault, env = {}) {
  const child = spawn(bin, ["mcp", "--vault", vault], {
    stdio: ["pipe", "pipe", "inherit"],
    env: { ...process.env, ...env },
  });
  const pending = new Map();
  let seq = 0;
  createInterface({ input: child.stdout }).on("line", (line) => {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    const waiting = pending.get(message.id);
    if (!waiting) return;
    pending.delete(message.id);
    if (message.error) waiting.reject(new Error(message.error.message));
    else waiting.resolve(message.result);
  });
  const request = (method, params) =>
    new Promise((resolve, reject) => {
      const id = ++seq;
      pending.set(id, { resolve, reject });
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    });

  const ready = request("initialize", {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "herbarium-tools", version: "1" },
  });

  /** Run one core operation (`area.verb`, also `area_verb`); reject with its message. */
  async function call(name, args = {}) {
    await ready;
    const result = await request("tools/call", { name: name.replaceAll(".", "_"), arguments: args ?? {} });
    const text = result?.content?.[0]?.text ?? "null";
    if (result?.isError) throw new Error(text);
    return JSON.parse(text);
  }

  return { ready, call, close: () => child.kill() };
}
