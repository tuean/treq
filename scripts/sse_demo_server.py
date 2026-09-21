#!/usr/bin/env python3
"""SSE 演示 / 自测服务（只用标准库，无需装依赖）。

    python3 scripts/sse_demo_server.py            # 默认 127.0.0.1:8787，每秒一条，永不结束
    python3 scripts/sse_demo_server.py --port 9000 --delay 0.3

端点：
    GET  /sse?n=5&delay=0.5   收 5 条就正常关闭（n=0 = 不收口，默认）
    POST /sse                 同样流式，body 原样回显在第一条事件里

在 treq 里试：Method=GET/POST，URL=http://127.0.0.1:8787/sse，
Header 加 `Accept: text/event-stream`，点发送 —— 响应区会边收边显示，发送按钮变成「停止」。
"""

import argparse
import json
import socket
import sys
import time
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"  # 流式响应：不写 Content-Length，读到连接关闭为止
    server_version = "treq-sse-demo"

    def log_message(self, fmt, *args):  # 默认日志太吵，改成每条连接一行
        pass

    def _args(self):
        q = parse_qs(urlparse(self.path).query)
        n = int(q.get("n", ["0"])[0])  # 0 = 不收口
        delay = float(q.get("delay", ["1"])[0])
        return max(0, n), max(0.0, delay)

    def _start_stream(self):
        self.connection.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream; charset=utf-8")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()

    def _write(self, text):
        self.wfile.write(text.encode())
        self.wfile.flush()

    def _stream(self, n, delay, first=None):
        addr = self.client_address[0]
        print(f"[sse] {addr} 连上 n={n or '∞'} delay={delay}s", flush=True)
        i = 0
        try:
            if first is not None:
                i += 1
                self._write(f"id: {i}\nevent: echo\ndata: {json.dumps(first, ensure_ascii=False)}\n\n")
            while n == 0 or i < n:
                i += 1
                payload = {
                    "seq": i,
                    "at": datetime.now().strftime("%H:%M:%S"),
                    "msg": f"第 {i} 条推送",
                    "items": [i, i * 2, i * 3],
                }
                self._write(f"id: {i}\nevent: tick\ndata: {json.dumps(payload, ensure_ascii=False)}\n\n")
                # 每 5 条来一个「多行 data」事件（data: 分行写，客户端拼成一段）
                if i % 5 == 0:
                    self._write(
                        "event: notice\ndata: 多行 data 演示\n"
                        f"data: 第 {i} 条了\n\n"
                    )
                # 每 3 条一个注释心跳（: 开头的行客户端要忽略，用来保活）
                if i % 3 == 0:
                    self._write(f": ping {i}\n\n")
                if n == 0 or i < n:
                    time.sleep(delay)
            print(f"[sse] {addr} 正常收口，共 {i} 条", flush=True)
        except (BrokenPipeError, ConnectionResetError):
            print(f"[sse] {addr} 客户端断开（共发出 {i} 条）", flush=True)
        finally:
            # 流式响应没有 Content-Length，收口时靠关连接让客户端知道「结束了」
            self.close_connection = True

    def do_GET(self):
        if urlparse(self.path).path != "/sse":
            self.send_error(404, "only /sse")
            return
        n, delay = self._args()
        self._start_stream()
        self._stream(n, delay)

    def do_POST(self):
        if urlparse(self.path).path != "/sse":
            self.send_error(404, "only /sse")
            return
        n, delay = self._args()
        body = self.rfile.read(int(self.headers.get("Content-Length") or 0)).decode("utf-8", "replace")
        self._start_stream()
        echo = {"method": "POST", "body": body, "server的时间": datetime.now().strftime("%H:%M:%S")}
        self._stream(n, delay, first=echo)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=8787)
    ap.add_argument("--delay", type=float, default=1.0, help="两条事件之间隔多少秒")
    a = ap.parse_args()
    srv = ThreadingHTTPServer((a.host, a.port), Handler)
    print(f"[sse] http://{a.host}:{a.port}/sse?n=0&delay={a.delay}  （Ctrl-C 停止）", flush=True)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        print("\n[sse] 停了", flush=True)
        sys.exit(0)


if __name__ == "__main__":
    main()
