# Shard Launcher MCP server

The MCP server connects to Shard Launcher over a loopback-only HTTP API.
The API is disabled unless `SHARD_API_TOKEN` is set in the launcher's environment.

1. Copy `.env.example` to `.env` and set a long random token.
2. Set the same `SHARD_API_TOKEN` in the environment used to start Shard Launcher.
3. Run `npm ci` and `npm run build`.
4. Configure your MCP client to run `node` with the absolute path to `dist/index.js`.

Do not commit `.env`; it contains the local API token.
