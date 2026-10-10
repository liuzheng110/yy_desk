#!/usr/bin/env python3
"""Mock Streamable HTTP MCP server: POST /mcp, plain JSON responses.

Usage: mock_http_mcp.py <port>
Tools: add(a, b) -> a+b
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(length)
        try:
            msg = json.loads(raw)
        except json.JSONDecodeError:
            msg = {}
        method = msg.get("method", "")
        rid = msg.get("id")

        if method == "initialize":
            out = {
                "jsonrpc": "2.0", "id": rid,
                "result": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "serverInfo": {"name": "mock-http", "version": "1.0"},
                },
            }
        elif method == "notifications/initialized":
            out = {}
        elif method == "ping":
            out = {"jsonrpc": "2.0", "id": rid, "result": {}}
        elif method == "tools/list":
            out = {
                "jsonrpc": "2.0", "id": rid,
                "result": {
                    "tools": [
                        {
                            "name": "add",
                            "description": "add two numbers",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "a": {"type": "number"},
                                    "b": {"type": "number"},
                                },
                                "required": ["a", "b"],
                            },
                        }
                    ]
                },
            }
        elif method == "tools/call":
            args = msg.get("params", {}).get("arguments", {})
            a = args.get("a", 0)
            b = args.get("b", 0)
            out = {
                "jsonrpc": "2.0", "id": rid,
                "result": {"content": [{"type": "text", "text": str(a + b)}]},
            }
        else:
            out = {
                "jsonrpc": "2.0", "id": rid,
                "error": {"code": -32601, "message": "method not found: " + method},
            }

        body = json.dumps(out).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 19001
    HTTPServer(("127.0.0.1", port), Handler).serve_forever()


if __name__ == "__main__":
    main()
