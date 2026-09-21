#!/usr/bin/env python3
"""treq 本地测试 mock server（默认 127.0.0.1:8321）。

路由：
  GET  /json                 固定 JSON（含中文）
  GET  /echo                 回显 method/url；POST /echo 还会回显 body 与 Content-Type
  POST /status/500           500
  GET  /status/404           404
  GET  /status?code=N        任意状态码
  GET  /delay                睡 1.5 秒（测超时）
  GET  /big?mb=30            约 30MB 的 JSON（测大响应护栏 / 保存）
  GET  /sse?n=20&gap=0.4     事件流
  ANY  /auth                 回显 Authorization / X-Api-Key / api_key 查询参数
  ANY  /cookies              回显 Cookie 头
  其它                        200 {"path": ...}
每个响应都会带 Set-Cookie: treq_session=demo123; Path=/; HttpOnly 与 lang=zh-CN; Max-Age=3600。
"""
import json
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = 8321


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _body(self):
        n = int(self.headers.get("Content-Length", 0))
        return self.rfile.read(n).decode("utf-8", "replace") if n else ""

    def _reply(self, status, payload):
        data = json.dumps(payload, ensure_ascii=False).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Set-Cookie", "treq_session=demo123; Path=/; HttpOnly")
        self.send_header("Set-Cookie", "lang=zh-CN; Path=/; Max-Age=3600")
        self.send_header("X-Mock-Server", "treq-test")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _sse(self, n, gap):
        """事件流：默认 20 个事件、每个间隔 0.4s（共 8s），方便观察边收边显示与「停止」。"""
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("X-Mock-Server", "treq-test")
        self.end_headers()
        for i in range(1, n + 1):
            payload = json.dumps({"seq": i, "ts": int(time.time() * 1000), "msg": f"事件 {i}"}, ensure_ascii=False)
            self.wfile.write(f"id: {i}\nevent: tick\ndata: {payload}\n\n".encode())
            self.wfile.flush()
            time.sleep(gap)
        self.wfile.write(b"event: done\ndata: {\"ok\": true}\n\n")
        self.wfile.flush()

    def do_GET(self):
        from urllib.parse import parse_qs, urlparse

        u = urlparse(self.path)
        path_only = u.path
        q = parse_qs(u.query)
        if path_only == "/sse":
            # /sse?n=20&gap=0.4
            self._sse(int(q.get("n", ["20"])[0]), float(q.get("gap", ["0.4"])[0]))
        elif path_only == "/big":
            # 大响应：核对响应区虚拟列表 / 大响应护栏
            # /big?mb=30 → 约 30 MB（默认 800 条约 50 KB）
            mb = int(q.get("mb", ["0"])[0] or 0)
            n = 800 if mb <= 0 else max(1, int(mb * 1024 * 1024 / 110))
            self._reply(200, {"items": [{"id": i, "name": f"接口-{i}", "url": f"/api/{i}", "note": "一些说明文本用来凑体积"} for i in range(1, n + 1)]})
        elif self.path == "/json":
            self._reply(200, {"hello": "world", "nested": {"n": 42}, "ok": True, "msg": "你好，treq"})
        elif self.path == "/delay":
            time.sleep(1.5)
            self._reply(200, {"delayed": True})
        elif self.path == "/status/404":
            self._reply(404, {"error": "not found"})
        elif path_only == "/auth":
            self._reply(200, {
                "authorization": self.headers.get("Authorization") or "",
                "x_api_key": self.headers.get("X-Api-Key") or "",
                "api_key_query": q.get("api_key", [""])[0],
                "all_headers": {k: v for k, v in self.headers.items() if k.lower().startswith("x-")},
            })
        elif path_only == "/cookies":
            self._reply(200, {"cookie_header": self.headers.get("Cookie") or ""})
        elif self.path == "/echo":
            self._reply(200, {"method": "GET", "url": self.path})
        else:
            self._reply(200, {"path": self.path})

    def do_POST(self):
        body = self._body()
        if self.path == "/echo":
            self._reply(200, {"method": "POST", "path": self.path, "body": body, "ct": self.headers.get("Content-Type")})
        elif self.path == "/status/500":
            self._reply(500, {"error": "server exploded"})
        else:
            self._reply(201, {"created": True, "path": self.path, "body": body})

    def do_PUT(self):
        self._reply(200, {"method": "PUT", "body": self._body()})

    def do_DELETE(self):
        self._reply(200, {"method": "DELETE"})

    def do_HEAD(self):
        self.send_response(200)
        self.send_header("Content-Length", "0")
        self.end_headers()


if __name__ == "__main__":
    print(f"treq mock server on http://127.0.0.1:{PORT}")
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()