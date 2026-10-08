// End to end: `herbarium publish` against a fake GitHub API. A page goes out
// as a secret gist (then updated, then deleted) and onto a GitHub Pages site
// (repository created with its marker, file written, Pages turned on, index
// rewritten, then the page taken down), with the vault keeping the records.
// A repository Herbarium did not make is never written to.
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { cleanup, herbariumBin, tempDir } from "./helpers.mjs";

let bin;
const dirs = [];

before(() => {
  bin = herbariumBin();
});
after(() => cleanup(...dirs));

/** Run the binary without blocking the event loop (the fake API lives in it). */
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

/** An in-memory GitHub: one user, gists, repositories with files, Pages. */
async function fakeGitHub(token) {
  const state = { gists: new Map(), repos: new Map(), log: [], nextGist: 1 };
  const server = createServer(async (req, res) => {
    let raw = "";
    for await (const chunk of req) raw += chunk;
    const body = raw ? JSON.parse(raw) : null;
    const url = new URL(req.url, "http://x");
    const path = decodeURIComponent(url.pathname);
    state.log.push(`${req.method} ${path}`);
    const send = (status, value) => {
      res.writeHead(status, { "content-type": "application/json" });
      res.end(value === undefined ? "" : JSON.stringify(value));
    };
    if (req.headers.authorization !== `Bearer ${token}`) return send(401, { message: "Bad credentials" });
    assert.equal(req.headers["x-github-api-version"], "2022-11-28");
    assert.match(req.headers["user-agent"], /^Herbarium\//);

    let m;
    if (req.method === "GET" && path === "/user") return send(200, { login: "octo" });
    if (req.method === "POST" && path === "/gists") {
      const id = `a${state.nextGist++}`; // hexadecimal, like real gist ids
      state.gists.set(id, body);
      return send(201, { id, html_url: `https://gist.github.com/octo/${id}` });
    }
    if ((m = path.match(/^\/gists\/(\w+)$/))) {
      if (!state.gists.has(m[1])) return send(404, { message: "Not Found" });
      if (req.method === "PATCH") {
        state.gists.set(m[1], body);
        return send(200, { id: m[1], html_url: `https://gist.github.com/octo/${m[1]}` });
      }
      if (req.method === "DELETE") {
        state.gists.delete(m[1]);
        return send(204);
      }
    }
    if (req.method === "POST" && path === "/user/repos") {
      state.repos.set(`octo/${body.name}`, { files: new Map([["README.md", "x"]]), pages: false, init: body.auto_init });
      return send(201, { full_name: `octo/${body.name}`, default_branch: "main" });
    }
    if ((m = path.match(/^\/repos\/(\w+\/[\w.-]+)(\/.*)?$/))) {
      const repo = state.repos.get(m[1]);
      const rest = m[2] ?? "";
      if (!repo) return send(404, { message: "Not Found" });
      if (rest === "" && req.method === "GET") return send(200, { default_branch: "main" });
      if (rest === "/pages") {
        if (req.method === "GET") return repo.pages ? send(200, { status: "built" }) : send(404, { message: "Not Found" });
        if (req.method === "POST") {
          assert.deepEqual(body.source, { branch: "main", path: "/" });
          repo.pages = true;
          return send(201, {});
        }
      }
      const file = rest.match(/^\/contents\/(.+)$/)?.[1];
      if (file) {
        const sha = (f) => `sha-${Buffer.from(repo.files.get(f)).length}`;
        if (req.method === "GET") return repo.files.has(file) ? send(200, { sha: sha(file) }) : send(404, { message: "Not Found" });
        if (req.method === "PUT") {
          if (repo.files.has(file)) assert.equal(body.sha, sha(file), `PUT ${file} names the current sha`);
          else assert.equal(body.sha, undefined);
          repo.files.set(file, Buffer.from(body.content, "base64").toString("utf8"));
          return send(repo.files.has(file) ? 200 : 201, { content: { path: file } });
        }
        if (req.method === "DELETE") {
          assert.equal(body.sha, sha(file));
          repo.files.delete(file);
          return send(200, {});
        }
      }
    }
    send(500, { message: `fake GitHub does not know ${req.method} ${path}` });
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  return { state, server, base: `http://127.0.0.1:${server.address().port}` };
}

test("herbarium publish shares a page as a gist and on a GitHub Pages site", async () => {
  const work = tempDir("publish");
  dirs.push(work);
  const vault = join(work, "vault");
  const gh = await fakeGitHub("t0ken");
  const env = {
    GITHUB_TOKEN: "t0ken",
    HERBARIUM_GITHUB_API: gh.base,
    HOME: work,
    XDG_CONFIG_HOME: join(work, "config"),
  };
  try {
    writeFileSync(join(work, "notes.html"), "<!doctype html><title>Rust notes</title><img src='pic.svg'><p>hi</p>");
    let r = await run(["add", "--vault", vault, join(work, "notes.html")], env);
    assert.equal(r.code, 0, r.stderr);
    const id = r.stdout.split("\t")[0];
    // A local image next to the page, inlined when published.
    writeFileSync(join(vault, "pic.svg"), "<svg xmlns='http://www.w3.org/2000/svg'/>");

    // A wrong token is reported as such.
    r = await run(["publish", "--vault", vault, id], { ...env, GITHUB_TOKEN: "nope" });
    assert.equal(r.code, 1);
    assert.match(r.stderr, /refused the token/);

    // Gist: created secret, then updated in place, then deleted.
    r = await run(["publish", "--vault", vault, "--gist", id], env);
    assert.equal(r.code, 0, r.stderr);
    assert.equal(r.stdout.trim(), "https://gist.github.com/octo/a1");
    const gist = gh.state.gists.get("a1");
    assert.equal(gist.public, false);
    assert.match(gist.description, /^Rust notes/);
    assert.match(gist.files[`${id}.html`].content, /<p>hi<\/p>/);
    r = await run(["publish", "--vault", vault, id], env);
    assert.equal(r.stdout.trim(), "https://gist.github.com/octo/a1", "publishing again updates the same gist");
    assert.equal(gh.state.gists.size, 1);
    r = await run(["publish", "--vault", vault, "--unpublish", id], env);
    assert.equal(r.code, 0, r.stderr);
    assert.equal(gh.state.gists.size, 0);

    // A repository Herbarium did not make is left alone.
    gh.state.repos.set("octo/mine", { files: new Map([["README.md", "mine"]]), pages: false });
    r = await run(["publish", "--vault", vault, "--site", "--repo", "mine", id], env);
    assert.equal(r.code, 1);
    assert.match(r.stderr, /was not made by Herbarium/);
    assert.deepEqual([...gh.state.repos.get("octo/mine").files.keys()], ["README.md"]);

    // Site: the repository is created with its marker, the page (assets
    // inlined) and an index are written, and Pages is turned on.
    r = await run(["publish", "--vault", vault, "--site", "--repo", "notes-site", id], env);
    assert.equal(r.code, 0, r.stderr);
    assert.equal(r.stdout.trim(), `https://octo.github.io/notes-site/${id}.html`);
    const site = gh.state.repos.get("octo/notes-site");
    assert.ok(site.files.has(".herbarium") && site.files.has(".nojekyll"));
    assert.match(site.files.get(`${id}.html`), /src="data:image\/svg\+xml;base64,/);
    assert.match(site.files.get("index.html"), new RegExp(`<a href="${id}.html">Rust notes</a>`));
    assert.equal(site.pages, true);

    // Again: the file is updated (with its sha) and Pages is not re-enabled.
    gh.state.log.length = 0;
    r = await run(["publish", "--vault", vault, "--site", "--repo", "notes-site", id], env);
    assert.equal(r.code, 0, r.stderr);
    assert.ok(!gh.state.log.includes("POST /user/repos"));
    assert.ok(!gh.state.log.includes("POST /repos/octo/notes-site/pages"));

    r = await run(["publish", "--vault", vault, "--site", "--unpublish", id], env);
    assert.equal(r.code, 0, r.stderr);
    assert.ok(!site.files.has(`${id}.html`));
    assert.doesNotMatch(site.files.get("index.html"), /Rust notes/);
  } finally {
    gh.server.close();
  }
});
