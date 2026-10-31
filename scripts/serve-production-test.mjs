// Serve the built files under a project-site prefix without SPA fallback, as on Pages.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, resolve, sep } from "node:path";

const root = resolve("dist");
const prefix = "/osdbits-wasm/";
const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm", ".map": "application/json" };
createServer(async (req, res) => {
  try {
    const pathname = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
    if (pathname === prefix.slice(0, -1)) {
      res.writeHead(301, { Location: prefix }).end();
      return;
    }
    if (!pathname.startsWith(prefix)) throw new Error("Not found");
    const path = resolve(root, pathname.slice(prefix.length) || "index.html");
    if (!path.startsWith(root + sep)) throw new Error("Not found");
    const body = await readFile(path);
    res.writeHead(200, { "Content-Type": types[extname(path)] || "application/octet-stream" }).end(body);
  } catch {
    res.writeHead(404).end("Not found");
  }
}).listen(4173, "127.0.0.1");
