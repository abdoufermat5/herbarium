// End to end: `herbarium remix` against a fake Anthropic Messages API (an
// event stream, as the real one sends) and a fake `claude` command. The
// remixed page waits as a proposal; the page itself is untouched. Refusals,
// bad keys and answers without HTML leave nothing behind.
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { cleanup, herbariumBin, tempDir, vaultPages } from "./helpers.mjs";

let bin;
const dirs = [];

before(() => {
  bin = herbariumBin();
});
after(() => cleanup(...dirs));

function run(args, env) {
  return new Promise((resolve) => {
    const child = spawn(bin, args, { env: { ...process.env, ...env } });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (d) => (stdout += d));
    child.stderr.on("data", (d) => (stderr += d));
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

const events = (list) => list.map((e) => `event: ${e.type}\ndata: ${JSON.stringify(e)}\n\n`).join("");

/** A Messages API that streams `next.reply` (text) and stops with `next.stop`. */
async function fakeApi() {
  const state = { requests: [], next: { reply: "", stop: "end_turn" } };
  const server = createServer(async (req, res) => {
    let raw = "";
    for await (const chunk of req) raw += chunk;
    if (req.method === "GET" && req.url.startsWith("/v1/models")) {
      // The model list, newest first, as Anthropic sends it.
      state.listed = (state.listed ?? 0) + 1;
      res.writeHead(req.headers["x-api-key"] === "sk-good" ? 200 : 401, { "content-type": "application/json" });
      return res.end(
        JSON.stringify({
          data: [
            { type: "model", id: "claude-opus-5-5", display_name: "Claude Opus 5.5" },
            { type: "model", id: "claude-sonnet-5-5", display_name: "Claude Sonnet 5.5" },
          ],
          has_more: false,
        }),
      );
    }
    state.requests.push({ headers: req.headers, body: JSON.parse(raw) });
    if (req.headers["x-api-key"] !== "sk-good") {
      res.writeHead(401, { "content-type": "application/json" });
      return res.end(JSON.stringify({ type: "error", error: { type: "authentication_error", message: "invalid x-api-key" } }));
    }
    const { reply, stop } = state.next;
    const half = Math.floor(reply.length / 2);
    res.writeHead(200, { "content-type": "text/event-stream" });
    res.end(
      events([
        { type: "message_start", message: { model: "claude-opus-5-5" } },
        { type: "content_block_start", index: 0, content_block: { type: "thinking", thinking: "" } },
        { type: "content_block_delta", index: 0, delta: { type: "thinking_delta", thinking: "Planning." } },
        { type: "content_block_stop", index: 0 },
        { type: "content_block_start", index: 1, content_block: { type: "text", text: "" } },
        { type: "content_block_delta", index: 1, delta: { type: "text_delta", text: reply.slice(0, half) } },
        { type: "ping" },
        { type: "content_block_delta", index: 1, delta: { type: "text_delta", text: reply.slice(half) } },
        { type: "content_block_stop", index: 1 },
        { type: "message_delta", delta: { stop_reason: stop } },
        { type: "message_stop" },
      ]),
    );
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  return { state, server, base: `http://127.0.0.1:${server.address().port}` };
}

const remixed = (title) =>
  `<!doctype html><html><head><title>${title}</title></head><body><p data-herbarium-recall="What is it?">It.</p></body></html>`;

test("herbarium remix leaves the model's page as a proposal", async () => {
  const work = tempDir("remix");
  dirs.push(work);
  const vault = join(work, "vault");
  const api = await fakeApi();
  const env = { HOME: work, XDG_CONFIG_HOME: join(work, "config"), HERBARIUM_ANTHROPIC_API: api.base, ANTHROPIC_API_KEY: "sk-good" };
  try {
    writeFileSync(join(work, "lesson.html"), "<!doctype html><title>Lesson</title><p>Original text</p>");
    let r = await run(["add", "--vault", vault, join(work, "lesson.html")], env);
    assert.equal(r.code, 0, r.stderr);
    const id = r.stdout.split("\t")[0];
    const proposal = join(vault, ".herbarium", "proposals", `${id}.html`);

    api.state.next = { reply: `Here is your quiz:\n\`\`\`html\n${remixed("Lesson quiz")}\n\`\`\`\n`, stop: "end_turn" };
    r = await run(["remix", "--vault", vault, "--preset", "quiz", "--instructions", "Five questions", id], env);
    assert.equal(r.code, 0, r.stderr);
    assert.match(r.stdout, /proposal ready for .*: Lesson quiz/);
    assert.equal(readFileSync(proposal, "utf8"), remixed("Lesson quiz"));
    const page = vaultPages(vault).find((p) => p.id === id);
    assert.match(page.html, /Original text/, "the page waits for approval");

    const { headers, body } = api.state.requests.at(-1);
    assert.equal(headers["anthropic-version"], "2023-06-01");
    assert.equal(headers["anthropic-beta"], "server-side-fallback-2026-07-01");
    assert.equal(body.model, "claude-opus-5-5", "no model set: the newest Opus from the service's list");
    assert.equal(api.state.listed, 1);
    assert.equal(body.stream, true);
    assert.deepEqual(body.thinking, { type: "adaptive" });
    assert.equal(body.fallbacks, "default");
    assert.match(body.system, /```html fenced code block/);
    const prompt = body.messages[0].content;
    assert.match(prompt, /data-herbarium-recall/);
    assert.match(prompt, /Five questions/);
    assert.match(prompt, /<page title="Lesson">\n<!doctype html><title>Lesson<\/title><p>Original text<\/p>\n<\/page>/);

    // A refusal, a bad key, and an answer without a page leave the proposal as it was.
    api.state.next = { reply: "partial", stop: "refusal" };
    r = await run(["remix", "--vault", vault, id], env);
    assert.equal(r.code, 1);
    assert.match(r.stderr, /declined/);
    r = await run(["remix", "--vault", vault, id], { ...env, ANTHROPIC_API_KEY: "sk-bad" });
    assert.equal(r.code, 1);
    assert.match(r.stderr, /key was refused/);
    api.state.next = { reply: "I can't find a page to change.", stop: "end_turn" };
    r = await run(["remix", "--vault", vault, "--model", "claude-sonnet-5-5", id], env);
    assert.equal(r.code, 1);
    assert.match(r.stderr, /no HTML page/);
    assert.equal(api.state.requests.at(-1).body.model, "claude-sonnet-5-5");
    assert.equal(readFileSync(proposal, "utf8"), remixed("Lesson quiz"));

    // Custom instructions are required for a custom remix.
    r = await run(["remix", "--vault", vault, "--preset", "custom", id], env);
    assert.equal(r.code, 1);
    assert.match(r.stderr, /say how to remix/);

    // Claude Code: the prompt goes on stdin of `claude -p`.
    if (process.platform !== "win32") {
      const fake = join(work, "claude");
      writeFileSync(
        fake,
        `#!/bin/sh\necho "$@" > '${work}/claude.args'\ncat > '${work}/claude.stdin'\nprintf '%s\\n' '\`\`\`html' '${remixed("Lesson in French")}' '\`\`\`'\n`,
        { mode: 0o755 },
      );
      r = await run(["remix", "--vault", vault, "--claude-code", "--preset", "translate", "--instructions", "French", id], {
        ...env,
        ANTHROPIC_API_KEY: "",
        HERBARIUM_CLAUDE_BIN: fake,
      });
      assert.equal(r.code, 0, r.stderr);
      assert.equal(readFileSync(join(work, "claude.args"), "utf8").trim(), "-p --output-format text --model opus --disallowedTools Bash,Edit,MultiEdit,Write,Read,Glob,Grep,LS,NotebookEdit,NotebookRead,WebFetch,WebSearch,Task,Agent,TodoWrite");
      assert.match(readFileSync(join(work, "claude.stdin"), "utf8"), /Translate this page[\s\S]*French[\s\S]*Original text/);
      assert.equal(readFileSync(proposal, "utf8"), remixed("Lesson in French"), "a newer remix replaces the proposal");
    }
    assert.ok(existsSync(proposal));
  } finally {
    api.server.close();
  }
});

/** An OpenAI-compatible chat completions server (DeepSeek, Gemini, Ollama…). */
async function fakeChatApi() {
  const state = { requests: [] };
  const server = createServer(async (req, res) => {
    let raw = "";
    for await (const chunk of req) raw += chunk;
    if (req.method === "GET" && req.url === "/v1/models") {
      state.listed = (state.listed ?? 0) + 1;
      res.writeHead(200, { "content-type": "application/json" });
      return res.end(JSON.stringify({ object: "list", data: [{ id: "deepseek-reasoner" }, { id: "deepseek-chat" }] }));
    }
    state.requests.push({ url: req.url, headers: req.headers, body: JSON.parse(raw) });
    if (req.headers.authorization && req.headers.authorization !== "Bearer ds-good") {
      res.writeHead(401, { "content-type": "application/json" });
      return res.end(JSON.stringify({ error: { message: "Authentication Fails", type: "authentication_error" } }));
    }
    const html = remixed(`Remixed by ${JSON.parse(raw).model}`);
    const chunks = [html.slice(0, 40), html.slice(40)];
    res.writeHead(200, { "content-type": "text/event-stream" });
    res.end(
      [
        { choices: [{ delta: { role: "assistant", content: "" } }] },
        { choices: [{ delta: { reasoning_content: "Planning the page." } }] },
        { choices: [{ delta: { content: "```html\n" + chunks[0] } }] },
        { choices: [{ delta: { content: chunks[1] + "\n```" }, finish_reason: "stop" }] },
      ]
        .map((e) => `data: ${JSON.stringify(e)}\n\n`)
        .join("") + "data: [DONE]\n\n",
    );
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  return { state, server, base: `http://127.0.0.1:${server.address().port}/v1` };
}

test("herbarium remix works with OpenAI-compatible services (DeepSeek, custom)", async () => {
  const work = tempDir("remix-chat");
  dirs.push(work);
  const vault = join(work, "vault");
  const api = await fakeChatApi();
  const env = { HOME: work, XDG_CONFIG_HOME: join(work, "config"), DEEPSEEK_API_KEY: "ds-good", HERBARIUM_AI_KEY: "" };
  try {
    writeFileSync(join(work, "lesson.html"), "<!doctype html><title>Lesson</title><p>Original text</p>");
    let r = await run(["add", "--vault", vault, join(work, "lesson.html")], env);
    assert.equal(r.code, 0, r.stderr);
    const id = r.stdout.split("\t")[0];
    const proposal = join(vault, ".herbarium", "proposals", `${id}.html`);

    // DeepSeek: its key from DEEPSEEK_API_KEY, its recommended model from its list.
    r = await run(["remix", "--vault", vault, "--provider", "deepseek", "--base-url", api.base, "--preset", "simplify", id], env);
    assert.equal(r.code, 0, r.stderr);
    assert.equal(readFileSync(proposal, "utf8"), remixed("Remixed by deepseek-chat"));
    assert.equal(api.state.listed, 1, "the model list was read");
    let req = api.state.requests.at(-1);
    assert.equal(req.url, "/v1/chat/completions");
    assert.equal(req.headers.authorization, "Bearer ds-good");
    assert.equal(req.body.stream, true);
    assert.equal(req.body.messages[0].role, "system");
    assert.match(req.body.messages[1].content, /plainer words[\s\S]*Original text/);

    r = await run(["remix", "--vault", vault, "--provider", "deepseek", "--base-url", api.base, id], { ...env, DEEPSEEK_API_KEY: "nope" });
    assert.equal(r.code, 1);
    assert.match(r.stderr, /key was refused: Authentication Fails/);

    // A custom service (a local server, no key), with the model named.
    r = await run(["remix", "--vault", vault, "--provider", "custom", "--base-url", api.base, "--model", "qwen2.5-coder", id], env);
    assert.equal(r.code, 0, r.stderr);
    req = api.state.requests.at(-1);
    assert.equal(req.headers.authorization, undefined);
    assert.equal(readFileSync(proposal, "utf8"), remixed("Remixed by qwen2.5-coder"));

    // Missing pieces are explained.
    r = await run(["remix", "--vault", vault, "--provider", "custom", id], env);
    assert.equal(r.code, 1);
    assert.match(r.stderr, /address of the API/);
    r = await run(["remix", "--vault", vault, "--provider", "gemini", id], env);
    assert.equal(r.code, 1);
    assert.match(r.stderr, /Google Gemini API key/);
    r = await run(["remix", "--vault", vault, "--provider", "custom", "--base-url", "http://example.com/v1", "--model", "m", id], env);
    assert.equal(r.code, 1);
    assert.match(r.stderr, /use https/);
  } finally {
    api.server.close();
  }
});
