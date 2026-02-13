#!/usr/bin/env node
// Scratch MCP stdio bridge
// Bridges JSON-RPC over stdin/stdout to Scratch's HTTP MCP endpoint.
// Claude Code spawns this script; it requires Node.js but zero npm dependencies.

import { createInterface } from "readline";
import http from "http";

const PORT = process.env.SCRATCH_MCP_PORT || 3921;
const HOST = "127.0.0.1";

function postToScratch(body) {
  return new Promise((resolve, reject) => {
    const data = JSON.stringify(body);
    const req = http.request(
      {
        hostname: HOST,
        port: PORT,
        path: "/mcp",
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(data),
        },
      },
      (res) => {
        let chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () => {
          try {
            resolve(JSON.parse(Buffer.concat(chunks).toString()));
          } catch {
            reject(new Error("Invalid JSON response from Scratch"));
          }
        });
      }
    );
    req.on("error", (err) => {
      reject(
        new Error(
          `Cannot connect to Scratch MCP server at ${HOST}:${PORT}. Is Scratch running with MCP enabled? (${err.message})`
        )
      );
    });
    req.write(data);
    req.end();
  });
}

const rl = createInterface({ input: process.stdin });

rl.on("line", async (line) => {
  if (!line.trim()) return;

  let request;
  try {
    request = JSON.parse(line);
  } catch {
    const error = {
      jsonrpc: "2.0",
      id: null,
      error: { code: -32700, message: "Parse error" },
    };
    process.stdout.write(JSON.stringify(error) + "\n");
    return;
  }

  try {
    const response = await postToScratch(request);
    process.stdout.write(JSON.stringify(response) + "\n");
  } catch (err) {
    const error = {
      jsonrpc: "2.0",
      id: request.id ?? null,
      error: { code: -32603, message: err.message },
    };
    process.stdout.write(JSON.stringify(error) + "\n");
  }
});

rl.on("close", () => process.exit(0));
