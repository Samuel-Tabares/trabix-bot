"""Meta y Anthropic falsos para correr el bot real en local sin gastar créditos.

El bot apunta acá con WHATSAPP_API_BASE y ANTHROPIC_API_URL. Todo lo que el bot
"envía" a WhatsApp queda en una bandeja de salida; cada llamada al modelo se
responde con el siguiente paso del guion que cargó el escenario.

Control (solo para el corredor de escenarios):
  POST /__script  {"responses": [...]}   carga el guion del próximo mensaje
  POST /__reset                          vacía bandeja, guion y registro
  GET  /__state                          bandeja, llamadas al modelo, guion sobrante

Formato de cada paso del guion:
  {"tools": [["add_order_item", {...}], ...]}   el "modelo" llama esas herramientas
  {"text": "..."}                               el "modelo" responde texto y termina
"""

import json
import os
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOCK = threading.Lock()
STATE: dict = {"outbox": [], "llm_calls": [], "script": [], "missing_script": 0, "brain_usage": [], "brain_errors": [], "brain_calls": []}
# script = guion fijo (gratis); claude = Claude Code con la suscripción (ver claude_brain.py)
BRAIN = os.environ.get("SIM_BRAIN", "script")
COUNTER = {"n": 0}


def _next_id(prefix: str) -> str:
    COUNTER["n"] += 1
    return f"{prefix}{COUNTER['n']}"


def _tool_results(request: dict) -> list[str]:
    """Texto de los tool_result del último mensaje del usuario (lo que el bot
    le devolvió al modelo): ahí se ve qué decidió la lógica determinista."""
    messages = request.get("messages") or []
    if not messages:
        return []
    last = messages[-1]
    content = last.get("content")
    if not isinstance(content, list):
        return []
    return [str(b.get("content", "")) for b in content if b.get("type") == "tool_result"]


def _anthropic_response(step: dict) -> dict:
    if "tools" in step:
        content = [
            {"type": "tool_use", "id": _next_id("toolu_sim_"), "name": name, "input": args}
            for name, args in step["tools"]
        ]
        stop = "tool_use"
    else:
        content = [{"type": "text", "text": step.get("text", "")}]
        stop = "end_turn"
    return {
        "id": _next_id("msg_sim_"),
        "type": "message",
        "role": "assistant",
        "content": content,
        "stop_reason": stop,
        "usage": {"input_tokens": 0, "output_tokens": 0,
                  "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0},
    }


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args) -> None:
        pass

    def _json(self, code: int, payload: dict) -> None:
        body = json.dumps(payload).encode()
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _body(self) -> dict:
        n = int(self.headers.get("content-length") or 0)
        raw = self.rfile.read(n) if n else b"{}"
        try:
            return json.loads(raw or b"{}")
        except json.JSONDecodeError:
            return {}

    def do_GET(self) -> None:
        if self.path == "/__state":
            with LOCK:
                self._json(200, STATE)
            return
        self._json(404, {"error": "not found"})

    def do_POST(self) -> None:
        if self.path == "/__reset":
            self._body()
            with LOCK:
                STATE.update(outbox=[], llm_calls=[], script=[], missing_script=0, brain_usage=[], brain_errors=[], brain_calls=[])
            self._json(200, {"ok": True})
            return
        if self.path == "/__script":
            data = self._body()
            with LOCK:
                STATE["script"] = list(data.get("responses") or [])
            self._json(200, {"ok": True})
            return
        if self.path.endswith("/messages") and self.path.startswith("/v21.0/"):
            data = self._body()
            text = (data.get("text") or {}).get("body") or json.dumps(data.get("interactive") or data.get("image") or data.get("document") or {}, ensure_ascii=False)
            with LOCK:
                STATE["outbox"].append({"to": data.get("to"), "type": data.get("type"), "text": text})
            self._json(200, {"messaging_product": "whatsapp", "messages": [{"id": _next_id("wamid.sim.")}]})
            return
        if self.path.endswith("/media") and self.path.startswith("/v21.0/"):
            self.rfile.read(int(self.headers.get("content-length") or 0))
            self._json(200, {"id": _next_id("media.sim.")})
            return
        if self.path == "/v1/messages":
            data = self._body()
            if BRAIN == "claude":
                import claude_brain
                with LOCK:
                    STATE["llm_calls"].append({"tool_results": _tool_results(data)})
                try:
                    response, usage = claude_brain.respond(data)
                except Exception as err:  # el bot verá un 500 y degradará como en producción
                    with LOCK:
                        STATE["brain_errors"].append(str(err)[:500])
                    self._json(500, {"type": "error", "error": {"type": "api_error", "message": str(err)[:300]}})
                    return
                with LOCK:
                    STATE["brain_usage"].append(usage)
                    for b in response["content"]:
                        if b["type"] == "tool_use":
                            STATE["brain_calls"].append({"name": b["name"], "input": b["input"]})
                        elif b["type"] == "text" and b["text"]:
                            STATE["brain_calls"].append({"name": "(texto al cliente)", "input": b["text"][:300]})
                self._json(200, response)
                return
            with LOCK:
                STATE["llm_calls"].append({"tool_results": _tool_results(data)})
                if STATE["script"]:
                    step = STATE["script"].pop(0)
                else:
                    STATE["missing_script"] += 1
                    step = {"text": "Listo."}
            self._json(200, _anthropic_response(step))
            return
        self._json(404, {"error": f"ruta no simulada: {self.path}"})


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 9900
    print(f"fake services en http://127.0.0.1:{port}", flush=True)
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
