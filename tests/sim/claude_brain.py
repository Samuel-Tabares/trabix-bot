"""Respuestas del "modelo" del bot usando Claude Code (la suscripción de Samuel)
en vez de la API de Anthropic.

El bot real arma su petición de Messages API (prompt, herramientas, historial) y
la manda a fake_services.py; con SIM_BRAIN=claude, fake_services se la pasa a
`claude -p` y traduce la respuesta a bloques `text` / `tool_use`.

No es idéntico a producción: Claude Code recibe las herramientas descritas en
texto y responde JSON, no `tool_use` nativo. Sirve para encontrar fallas de
comportamiento y de integración, no para medir latencia. Los tokens que reporta
`claude -p` sí son de este mismo prompt, así que sirven para estimar costo.
Nunca usar esto en producción: la suscripción es para uso personal, no para
atender clientes.
"""

from __future__ import annotations

import json
import os
import subprocess
import uuid

MODEL = os.environ.get("SIM_BRAIN_MODEL", "claude-sonnet-5")
TIMEOUT_S = int(os.environ.get("SIM_BRAIN_TIMEOUT", "180"))

RESPONSE_SCHEMA = {
    "type": "object",
    "properties": {
        "text": {"type": ["string", "null"], "description": "Texto para el cliente, o null si solo llamas herramientas."},
        "tool_calls": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {"name": {"type": "string"}, "input": {"type": "object"}},
                "required": ["name", "input"],
            },
        },
    },
    "required": ["text", "tool_calls"],
}

PROTOCOL = """

## PROTOCOLO DE ESTA SESIÓN (no es parte del negocio)
No tienes herramientas nativas. Para usar una herramienta del bot, inclúyela en
`tool_calls` con su nombre exacto y un `input` que cumpla su esquema. El sistema
la ejecuta y te devuelve el resultado en el siguiente mensaje. Puedes llamar
varias a la vez. Si solo llamas herramientas, `text` va en null. Cuando ya
tengas lo que necesitas, responde al cliente en `text` con `tool_calls` vacío.
Responde SIEMPRE con el JSON pedido, nada más.
"""


def _claude(system: str, prompt: str, schema: dict, model: str) -> dict:
    cmd = [
        "claude", "-p", "--model", model, "--output-format", "json",
        "--tools", "", "--system-prompt", system,
        "--json-schema", json.dumps(schema),
        "--no-session-persistence", "--strict-mcp-config",
        "--setting-sources", "", "--disable-slash-commands",
    ]
    out = subprocess.run(cmd, input=prompt, capture_output=True, text=True, timeout=TIMEOUT_S,
                         cwd=os.environ.get("SIM_CLAUDE_CWD", "/tmp"))
    if out.returncode != 0:
        raise RuntimeError(f"claude -p falló: {out.stderr.strip()[:500]}")
    data = json.loads(out.stdout)
    if data.get("is_error"):
        raise RuntimeError(f"claude -p devolvió error: {str(data.get('result'))[:500]}")
    return data


def _main_usage(data: dict, model: str) -> dict:
    """Uso del modelo principal (Claude Code también llama a uno chico interno)."""
    mu = data.get("modelUsage") or {}
    chosen = mu.get(model) or next((v for k, v in mu.items() if "haiku" not in k), None) or {}
    return {
        "model": chosen.get("canonicalModel", model),
        "input": chosen.get("inputTokens", 0),
        "cache_write": chosen.get("cacheCreationInputTokens", 0),
        "cache_read": chosen.get("cacheReadInputTokens", 0),
        "output": chosen.get("outputTokens", 0),
        "list_cost_usd": chosen.get("costUSD", 0.0),
    }


def _render_block(block: dict) -> str:
    t = block.get("type")
    if t == "text":
        return block.get("text", "")
    if t == "tool_use":
        return f"[llamaste {block['name']} con {json.dumps(block.get('input'), ensure_ascii=False)}]"
    if t == "tool_result":
        err = " (ERROR)" if block.get("is_error") else ""
        return f"[resultado{err}: {block.get('content')}]"
    return ""


def _render_messages(messages: list) -> str:
    lines = []
    for m in messages:
        content = m.get("content")
        if isinstance(content, str):
            body = content
        else:
            body = "\n".join(x for x in (_render_block(b) for b in content) if x)
        who = "TÚ (el bot)" if m.get("role") == "assistant" else "ENTRADA"
        if body.strip():
            lines.append(f"### {who}\n{body}")
    return "\n\n".join(lines)


def respond(request: dict) -> tuple[dict, dict]:
    """Devuelve (respuesta estilo Messages API, uso medido)."""
    system_blocks = request.get("system") or []
    if isinstance(system_blocks, str):
        system_blocks = [{"text": system_blocks}]
    # El primer bloque es el prompt estático del bot (el que se cachea en
    # producción); el resto es el estado dinámico del caso.
    static = system_blocks[0].get("text", "") if system_blocks else ""
    dynamic = "\n\n".join(b.get("text", "") for b in system_blocks[1:])
    tools = [
        {"name": t["name"], "description": t.get("description", ""), "input_schema": t.get("input_schema")}
        for t in request.get("tools") or []
    ]
    system = static + "\n\n## HERRAMIENTAS DISPONIBLES (JSON)\n" + json.dumps(tools, ensure_ascii=False) + PROTOCOL
    prompt = f"## ESTADO ACTUAL DEL CASO\n{dynamic}\n\n## CONVERSACIÓN\n{_render_messages(request.get('messages') or [])}"

    data = _claude(system, prompt, RESPONSE_SCHEMA, MODEL)
    out = data.get("structured_output") or {}
    content = []
    if out.get("text"):
        content.append({"type": "text", "text": out["text"]})
    for call in out.get("tool_calls") or []:
        content.append({"type": "tool_use", "id": f"toolu_cc_{uuid.uuid4().hex[:16]}",
                        "name": call.get("name", ""), "input": call.get("input") or {}})
    if not content:
        content = [{"type": "text", "text": ""}]
    stop = "tool_use" if any(b["type"] == "tool_use" for b in content) else "end_turn"
    usage = _main_usage(data, MODEL)
    return ({
        "id": f"msg_cc_{uuid.uuid4().hex[:12]}", "type": "message", "role": "assistant",
        "content": content, "stop_reason": stop,
        "usage": {"input_tokens": usage["input"], "output_tokens": usage["output"],
                  "cache_creation_input_tokens": usage["cache_write"], "cache_read_input_tokens": usage["cache_read"]},
    }, usage)


def ask_json(system: str, prompt: str, schema: dict, model: str | None = None) -> tuple[dict, dict]:
    """Llamada genérica (cliente simulado o juez) con salida JSON validada."""
    m = model or MODEL
    data = _claude(system, prompt, schema, m)
    return data.get("structured_output") or {}, _main_usage(data, m)
