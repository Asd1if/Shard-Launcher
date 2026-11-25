#!/usr/bin/env node
// Мост stdio → HTTP для MCP-клиентов, которые умеют запускать только локальные команды.
// Пересылает каждое JSON-RPC сообщение со stdin на встроенный MCP-сервер Shard и печатает ответ.
// Без зависимостей: нужен только Node.js 18+.
//
//   node shard-mcp.mjs "http://127.0.0.1:8542/mcp?token=…"
//   SHARD_MCP_URL="http://127.0.0.1:8542/mcp?token=…" node shard-mcp.mjs
import { createInterface } from "node:readline";

const url = process.argv[2] ?? process.env.SHARD_MCP_URL;
if (!url) {
  console.error("Usage: shard-mcp.mjs <MCP link from Shard → Settings → MCP server>");
  process.exit(2);
}

const write = (message) => process.stdout.write(JSON.stringify(message) + "\n");
const fail = (id, message) => write({ jsonrpc: "2.0", id: id ?? null, error: { code: -32000, message } });

async function forward(line) {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    return fail(null, "Parse error");
  }
  const id = Array.isArray(message) ? null : message.id;
  try {
    const response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json, text/event-stream" },
      body: line,
    });
    if (response.status === 202) return;
    if (!response.ok) {
      const hint = response.status === 401 ? " (the link is outdated — copy it again in Shard)" : "";
      return id !== undefined && fail(id, `Shard answered HTTP ${response.status}${hint}`);
    }
    const text = await response.text();
    if (text) process.stdout.write(text.trim() + "\n");
  } catch {
    if (id !== undefined) fail(id, "Shard is not running or the MCP server is off (Settings → MCP server)");
  }
}

// Сообщения обрабатываем по порядку: так ответы не перемешаются
let queue = Promise.resolve();
createInterface({ input: process.stdin }).on("line", (line) => {
  if (line.trim()) queue = queue.then(() => forward(line));
});
