#!/usr/bin/env python3
"""Local backend for CRC Lab: serves the web UI and compiles PDF reports.

One command for the whole demo:
    python3 report/server.py
then open http://127.0.0.1:8321/

POST /api/export-pdf  {mode, text, bits, gen, flips[]} -> application/pdf
POST /api/export-tex  {mode, text, bits, gen, flips[]} -> text/x-tex

Stdlib only. Binds loopback, validates every field, compiles in a temp dir.
"""

import json
import subprocess
import sys
import tempfile
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent
WEB_DIR = HERE.parent / "web"
PORT = 8321

sys.path.insert(0, str(HERE))
from export_report import (  # noqa: E402
    build_tex,
    check_frame,
    divide,
    parse_bits,
    text_to_bits,
)


def validate_payload(obj):
    """Return (label, data, gen, flips) or raise ValueError."""
    if not isinstance(obj, dict):
        raise ValueError("body must be JSON object")
    mode = obj.get("mode", "text")
    gen_text = str(obj.get("gen", ""))
    gen = parse_bits(gen_text)
    if len(gen_text) < 2 or len(gen_text) > 17 or set(gen_text) - {"0", "1"}:
        raise ValueError("generator must be 2-17 bits of 0/1")
    if gen[0] != 1 or gen[-1] != 1:
        raise ValueError("generator must start and end with 1")
    if mode == "text":
        label = str(obj.get("text", ""))
        if not 1 <= len(label) <= 12:
            raise ValueError("message must be 1-12 characters")
        data = text_to_bits(label)
    elif mode == "bits":
        label = str(obj.get("bits", ""))
        if not label or set(label) - {"0", "1"} or len(label) > 64:
            raise ValueError("data bits must be 1-64 chars of 0/1")
        data = parse_bits(label)
    else:
        raise ValueError("mode must be 'text' or 'bits'")
    raw_flips = obj.get("flips", [])
    if not isinstance(raw_flips, list) or len(raw_flips) > 64:
        raise ValueError("flips must be a list of at most 64 positions")
    frame_len = len(data) + len(gen) - 1
    flips = []
    for p in raw_flips:
        if not isinstance(p, int) or not 0 <= p < frame_len:
            raise ValueError(f"flip position {p!r} out of range")
        flips.append(p)
    return label, data, gen, sorted(set(flips))


def compute(label, data, gen, flips):
    crc, steps, augmented = divide(data, gen)
    frame = data + crc
    received = frame[:]
    for p in flips:
        received[p] ^= 1
    remainder, valid = check_frame(received, gen)
    tex = build_tex(label, data, gen, crc, frame, received, flips, remainder, valid, steps, augmented)
    return tex


def compile_pdf(tex):
    """Compile tex string to PDF bytes in a temp dir. Raises on failure."""
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        (d / "report.tex").write_text(tex)
        for _ in range(2):
            r = subprocess.run(
                ["pdflatex", "-interaction=nonstopmode", "report.tex"],
                cwd=d,
                capture_output=True,
                text=True,
                timeout=60,
            )
        log = (d / "report.log").read_text(errors="replace")
        errors = [line for line in log.splitlines() if line.startswith("!")]
        if errors or r.returncode != 0:
            raise RuntimeError("pdflatex failed: " + "; ".join(errors[:5]))
        return (d / "report.pdf").read_bytes()


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(WEB_DIR), **kwargs)

    def log_message(self, *args):
        pass  # keep viva terminal clean

    def _json_body(self):
        length = int(self.headers.get("Content-Length", 0))
        if length <= 0 or length > 4096:
            raise ValueError("empty or oversized body")
        return json.loads(self.rfile.read(length))

    def _send(self, data, content_type, filename):
        blob = bytes(data) if isinstance(data, (bytes, bytearray)) else data.encode()
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(blob)))
        self.send_header("Content-Disposition", f'attachment; filename="{filename}"')
        self.end_headers()
        self.wfile.write(blob)

    def _send_error_json(self, message):
        blob = json.dumps({"error": message}).encode()
        self.send_response(400)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(blob)))
        self.end_headers()
        self.wfile.write(blob)

    def do_POST(self):
        try:
            obj = self._json_body()
            label, data, gen, flips = validate_payload(obj)
            tex = compute(label, data, gen, flips)
        except (ValueError, json.JSONDecodeError) as e:
            self._send_error_json(str(e))
            return
        try:
            if self.path == "/api/export-pdf":
                self._send(compile_pdf(tex), "application/pdf", "crc-report.pdf")
            elif self.path == "/api/export-tex":
                self._send(tex, "text/x-tex", "crc-report.tex")
            else:
                self.send_error(404)
        except RuntimeError as e:
            self._send_error_json(str(e))
        except BrokenPipeError:
            pass


def main():
    server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    print(f"CRC Lab at http://127.0.0.1:{PORT}/  (Ctrl-C to stop)")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
