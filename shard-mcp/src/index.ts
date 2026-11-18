import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import * as dotenv from "dotenv";

dotenv.config({ path: resolve(dirname(fileURLToPath(import.meta.url)), "../.env") });

const API_URL = "http://127.0.0.1:8542";
const API_TOKEN = process.env.SHARD_API_TOKEN;

if (!API_TOKEN) {
  throw new Error("Set SHARD_API_TOKEN in the environment or shard-mcp/.env");
}

const headers = {
  Authorization: `Bearer ${API_TOKEN}`,
  "Content-Type": "application/json",
};
const instanceSchema = {
  instance_id: z.string(),
};

const server = new McpServer({
  name: "shard-launcher",
  version: "1.0.0",
});

async function callApi(
  path: string,
  init?: { method?: string; body?: string },
): Promise<unknown> {
  const response = await fetch(`${API_URL}${path}`, {
    ...init,
    headers,
  });
  const text = await response.text();
  let data: unknown;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    data = text;
  }

  if (!response.ok) {
    const message =
      typeof data === "object" &&
      data !== null &&
      "message" in data &&
      typeof data.message === "string"
        ? data.message
        : `HTTP ${response.status}`;
    throw new Error(message);
  }

  return data;
}

server.registerTool<{}, undefined>(
  "get_instances",
  {
    description:
      "Get a list of all configured Minecraft instances in Shard Launcher, including their IDs, names, and status.",
  },
  async (): Promise<CallToolResult> => {
    try {
      const data = await callApi("/api/instances");
      return {
        content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
      };
    } catch (error) {
      return {
        content: [{ type: "text", text: `Error fetching instances: ${error}` }],
        isError: true,
      };
    }
  },
);

server.registerTool<{}, typeof instanceSchema>(
  "launch_instance",
  {
    description:
      "Launch a specific Minecraft instance by its ID. Use get_instances first to find the correct ID.",
    inputSchema: {
      ...instanceSchema,
    },
  },
  async ({ instance_id }: { instance_id: string }): Promise<CallToolResult> => {
    try {
      const data = await callApi("/api/launch", {
        method: "POST",
        body: JSON.stringify({ instance_id }),
      });
      return {
        content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
      };
    } catch (error) {
      return {
        content: [{ type: "text", text: `Error launching instance: ${error}` }],
        isError: true,
      };
    }
  },
);

server.registerTool<{}, typeof instanceSchema>(
  "stop_instance",
  {
    description: "Stop a running Minecraft instance by its ID.",
    inputSchema: {
      ...instanceSchema,
    },
  },
  async ({ instance_id }: { instance_id: string }): Promise<CallToolResult> => {
    try {
      const data = await callApi("/api/stop", {
        method: "POST",
        body: JSON.stringify({ instance_id }),
      });
      return {
        content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
      };
    } catch (error) {
      return {
        content: [{ type: "text", text: `Error stopping instance: ${error}` }],
        isError: true,
      };
    }
  },
);

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error("Shard Launcher MCP Server running on stdio");
}

main().catch((error) => {
  console.error("Fatal error in main():", error);
  process.exitCode = 1;
});
