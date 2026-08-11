#!/usr/bin/env python3
"""
Simple webhook test server.

Listens for incoming HTTP requests and pretty-prints the headers + JSON body.

Usage:
    python3 webhook_server.py                  # listen on 0.0.0.0:8085
    python3 webhook_server.py --port 9000
    python3 webhook_server.py --fail-rate 0.5   # randomly return 500s (to test retry logic)
    python3 webhook_server.py --delay 3         # add artificial latency (seconds)
    python3 webhook_server.py --log-file events.jsonl  # also append payloads to a file

e.g.: http://127.0.0.1:8085/?webhook_events=tick
"""

import argparse
import json
import random
import time
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse


def make_handler(args: argparse.Namespace):
    """Build a WebhookHandler class with `args` bound via closure."""

    class WebhookHandler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def _log(self, msg):
            ts = datetime.now(timezone.utc).isoformat()
            print(f"[{ts}] {msg}")

        def _handle(self):
            parsed = urlparse(self.path)
            query = parse_qs(parsed.query)

            length = int(self.headers.get("Content-Length", 0))
            raw_body = self.rfile.read(length) if length else b""

            self._log(f"{self.command} {self.path}")
            self._log(f"  headers: {dict(self.headers)}")

            body_for_log = None
            if raw_body:
                try:
                    parsed_json = json.loads(raw_body)
                    body_for_log = parsed_json
                    pretty = json.dumps(parsed_json, indent=2)
                    print(pretty)
                except json.JSONDecodeError:
                    self._log(f"  raw body (not JSON): {raw_body!r}")
                    body_for_log = raw_body.decode("utf-8", errors="replace")
            else:
                self._log("  (empty body)")

            if query:
                self._log(f"  query params: {query}")

            if args.log_file:
                with open(args.log_file, "a") as f:
                    f.write(
                        json.dumps(
                            {
                                "received_at": datetime.now(timezone.utc).isoformat(),
                                "path": self.path,
                                "headers": dict(self.headers),
                                "body": body_for_log,
                            }
                        )
                        + "\n"
                    )

            if args.delay:
                time.sleep(args.delay)

            if args.fail_rate and random.random() < args.fail_rate:
                self._log("  -> simulating failure (500)")
                self.send_response(500)
                self.send_header("Content-Length", "0")
                self.end_headers()
                return

            response_body = b"{}"
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(response_body)))
            self.end_headers()
            self.wfile.write(response_body)

        # Handle any HTTP method the same way (webhook is usually POST, but
        # this is more forgiving for testing).
        def do_POST(self):
            self._handle()

        def do_GET(self):
            self._handle()

        def do_PUT(self):
            self._handle()

        # Quiet the default request logging; we do our own above.
        def log_message(self, format, *args):
            pass

    return WebhookHandler


def main():
    parser = argparse.ArgumentParser(description="Simple webhook test server")
    parser.add_argument(
        "--host", default="0.0.0.0", help="Host to bind (default: 0.0.0.0)"
    )
    parser.add_argument(
        "--port", type=int, default=8085, help="Port to bind (default: 8085)"
    )
    parser.add_argument(
        "--fail-rate",
        type=float,
        default=0.0,
        help="Probability (0-1) of returning a 500 response, to test your retry logic",
    )
    parser.add_argument(
        "--delay",
        type=float,
        default=0.0,
        help="Artificial delay in seconds before responding",
    )
    parser.add_argument(
        "--log-file",
        default=None,
        help="Optional path to append received payloads as JSON lines",
    )
    args = parser.parse_args()

    handler_cls = make_handler(args)
    server = ThreadingHTTPServer((args.host, args.port), handler_cls)
    print(f"Webhook test server listening on http://{args.host}:{args.port}")
    print(
        f"  fail_rate={args.fail_rate}  delay={args.delay}s  log_file={args.log_file}"
    )
    print("Press Ctrl+C to stop.\n")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down.")
        server.shutdown()


if __name__ == "__main__":
    main()
