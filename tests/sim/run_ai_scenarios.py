"""Suite con IA real, pagada por la suscripción de Claude Code (no por créditos
de API): un Claude hace de cliente de Armenia, el bot REAL responde con Claude
Code como cerebro (fake_services.py con SIM_BRAIN=claude), y otro Claude hace de
juez revisando la conversación, las herramientas y lo que quedó en la base.

Busca lo que la suite con guion no ve: cómo entiende el modelo lo que escribe
la gente, si elige bien las herramientas, si inventa cifras, si se enreda.

Uso:  python3 tests/sim/run_ai_scenarios.py            (todas las personas)
      python3 tests/sim/run_ai_scenarios.py P03 P07    (algunas)
Salida: docs/qa_ia/<fecha>/reporte.md + datos.json
"""

from __future__ import annotations

import datetime as dt
import importlib.util
import json
import os
import pathlib
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import claude_brain  # noqa: E402

_spec = importlib.util.spec_from_file_location("sim", HERE / "run_scenarios.py")
sim = importlib.util.module_from_spec(_spec)
_argv, sys.argv = sys.argv, ["x"]
_spec.loader.exec_module(sim)
sys.argv = _argv

ONLY = set(sys.argv[1:])
MAX_TURNS = int(os.environ.get("SIM_MAX_TURNS", "10"))
CUSTOMER_MODEL = os.environ.get("SIM_CUSTOMER_MODEL", "claude-sonnet-5-5")
JUDGE_MODEL = os.environ.get("SIM_JUDGE_MODEL", "claude-opus-5-5")
OUT_DIR = pathlib.Path(os.environ.get("SIM_REPORT_DIR", HERE.parents[2] / "docs" / "qa_ia")) / dt.datetime.now().strftime("%Y-%m-%d_%H%M")

# Precios por millón de tokens (verificados 2026-11-19). Escritura de caché a
# 1,25x la entrada (caché de 5 min, la que usa el bot).
PRICES = {
    "claude-sonnet-5 (actual)": {"in": 2.0, "out": 10.0, "read": 0.20, "write": 2.50},
    "claude-sonnet-5-5": {"in": 2.0, "out": 10.0, "read": 0.10, "write": 2.50},
    "claude-opus-5-5": {"in": 4.0, "out": 20.0, "read": 0.20, "write": 5.00},
}
COP_PER_USD = float(os.environ.get("SIM_COP_PER_USD", "4000"))

# team: qué contesta "el equipo" si el bot pregunta disponibilidad de un
# inmediato: True = "Sí, lo despacho", False = "No puedo", None = nadie (vence).
PERSONAS = [
    {"id": "P01", "nombre": "Valentina", "team": True,
     "perfil": "Estudiante de 22 años, vive en La Castellana (norte de Armenia). Escribe rápido, sin tildes.",
     "objetivo": "Pedir 3 granizados con licor (uno de uva, dos de maracumango) para ya, pagar contra entrega.",
     "esperado": "Domicilio norte $6.000 (menos de 6 unidades). Promo del segundo a mitad solo con licor. Pide confirmar disponibilidad; con 'Sí' confirma y pregunta pago."},
    {"id": "P02", "nombre": "Don Hernán", "team": True,
     "perfil": "Señor de 55 años en el barrio Granada (centro). Escribe formal y con algunos errores.",
     "objetivo": "Pedir 6 granizados surtidos con licor para ya, y preguntar cuánto le sale el domicilio.",
     "esperado": "Con 6 unidades en Armenia el domicilio es GRATIS. No debe cobrar domicilio ni inventar cifras."},
    {"id": "P03", "nombre": "Camilo", "team": None,
     "perfil": "Joven en La Clarita (sur). Indeciso: primero pide 6 y luego dice que mejor 3.",
     "objetivo": "Pedir 6 de uva vodka para ya, después cambiar a 3. Preguntar si sigue gratis el domicilio.",
     "esperado": "Con 6 es gratis; al bajar a 3 debe cobrar $10.000 (sur) y decírselo. Si nadie confirma disponibilidad, se le ofrece programar."},
    {"id": "P04", "nombre": "Marcela", "team": True,
     "perfil": "Tiene una tienda en Armenia y quiere revender. Pregunta precios por mayor.",
     "objetivo": "Comprar 30 sin licor para revender (variados), entrega en el barrio Los Profesionales, pagar por transferencia.",
     "esperado": "Mayorista sin licor 20-49 a $4.800 c/u; domicilio norte $6.000 se cobra; debe preguntar por código de referido antes de cerrar; transferencia."},
    {"id": "P05", "nombre": "Andrés", "team": True,
     "perfil": "Vive en Calarcá, en el barrio Granada de Calarcá (no de Armenia). No aclara de entrada que es Calarcá.",
     "objetivo": "Pedir 6 granizados con licor. Cuando le pregunten la dirección dice 'barrio Granada, Calarcá'.",
     "esperado": "Calarcá NO es Armenia: domicilio $15.000, nunca gratis, sin mínimo de unidades."},
    {"id": "P06", "nombre": "Laura", "team": True,
     "perfil": "Vive en Pereira. Quiere 25 granizados para una fiesta.",
     "objetivo": "Pedir 25 con licor enviados a Pereira.",
     "esperado": "Envío nacional por transportadora: mínimo 20, llega DESCONGELADO (debe decirlo), solo transferencia; el flete lo cotiza un asesor (pasa a humano)."},
    {"id": "P07", "nombre": "Sebastián", "team": True,
     "perfil": "Pregunta de todo antes de comprar: precios, si hay sin alcohol, cuánto dura congelado. Termina sin comprar.",
     "objetivo": "Averiguar precios con y sin licor y si el segundo sale a mitad también sin licor. No compra.",
     "esperado": "Con licor $8.000 y el segundo a mitad (2 = $12.000); sin licor $7.000 sin promo (2 = $14.000). No inventar cifras."},
    {"id": "P08", "nombre": "Kevin", "team": True,
     "perfil": "Escribe con muchas faltas y abreviaturas tipo 'kiero 4 granisados d mango con ron pa la patria'.",
     "objetivo": "Pedir 4 de maracumango con ron para el barrio La Patria, ya, contra entrega.",
     "esperado": "Entender el pedido (maracumango ron x4), barrio La Patria = sur $10.000, confirmar disponibilidad."},
    {"id": "P09", "nombre": "Daniela", "team": True,
     "perfil": "Organiza un cumpleaños infantil mañana. Necesita sin alcohol.",
     "objetivo": "Pedir 15 sin licor para mañana a las 3 pm en el barrio Fundadores, pagar contra entrega.",
     "esperado": "Programado (mínimo 3h, mañana sirve), sin licor $7.000 c/u sin promo, 15 unidades en Armenia: domicilio gratis. Se autoacepta sin preguntar disponibilidad."},
    {"id": "P10", "nombre": "Felipe", "team": False,
     "perfil": "Pide para ya en Unicentro. Tiene un código de un amigo embajador: 'jega1'.",
     "objetivo": "Pedir 4 con licor para ya y usar el código jega1 para descuento.",
     "esperado": "El código solo aplica al por mayor (20+ de un tipo): debe explicarlo, no aplicar descuento. Si el equipo dice 'No puedo', se le ofrece programar."},
    {"id": "P11", "nombre": "Gloria", "team": True,
     "perfil": "Señora que manda su ubicación de WhatsApp en vez de escribir la dirección.",
     "objetivo": "Pedir 2 granizados de uva para ya; cuando le pidan dirección manda su ubicación (pin) y luego, si le preguntan, dice 'barrio La Castellana'.",
     "esperado": "La ubicación no pasa el caso a humano: el bot pide barrio/dirección en texto y calcula domicilio norte $6.000.", "location_on_address": True},
    {"id": "P12", "nombre": "Mateo", "team": True,
     "perfil": "Impaciente, después de dos mensajes pide hablar con una persona de verdad.",
     "objetivo": "Preguntar por precios por mayor y luego exigir hablar con un humano.",
     "esperado": "Al pedir una persona, el bot pasa el caso a un humano y avisa al cliente (no intenta retenerlo)."},
]

CUSTOMER_SCHEMA = {
    "type": "object",
    "properties": {
        "message": {"type": "string"},
        "send_location": {"type": "boolean"},
        "done": {"type": "boolean"},
    },
    "required": ["message", "send_location", "done"],
}

JUDGE_SCHEMA = {
    "type": "object",
    "properties": {
        "veredicto": {"type": "string", "enum": ["bien", "con_problemas", "grave"]},
        "resumen": {"type": "string"},
        "problemas": {"type": "array", "items": {"type": "object", "properties": {
            "gravedad": {"type": "string", "enum": ["alta", "media", "baja"]},
            "que": {"type": "string"}, "evidencia": {"type": "string"}, "sugerencia": {"type": "string"}},
            "required": ["gravedad", "que", "evidencia", "sugerencia"]}},
    },
    "required": ["veredicto", "resumen", "problemas"],
}

BUSINESS_RULES = """REGLAS DEL NEGOCIO (Trabix Granizados, Armenia, Quindío):
- Detal con licor $8.000 c/u, el SEGUNDO a mitad de precio (2 = $12.000). Sin licor $7.000 c/u SIN promo (2 = $14.000).
- Mayorista desde 20 unidades del mismo tipo: con licor 20-49 $4.900, 50-99 $4.700, 100+ $4.500; sin licor 20-49 $4.800, 50-99 $4.500, 100+ $4.200.
- Domicilio Armenia por zona: norte $6.000, centro $8.000, sur $10.000. GRATIS solo en Armenia y solo de 6 a 19 unidades. 20+ siempre se cobra. Si cambia la cantidad, cambia el domicilio.
- El bot nunca adivina la zona: o la reconoce de una tabla de barrios, o pregunta.
- Pueblos: Calarcá $15.000, Circasia/Montenegro/La Tebaida $16.000 (sin mínimo); Salento, Filandia, Quimbaya, etc. mínimo 20. Fuera de Armenia nunca es gratis.
- Envío nacional (resto del país): mínimo 20, llega DESCONGELADO (hay que decirlo), solo transferencia, el flete lo cotiza un humano (el bot pasa el caso a humano).
- Código de embajador: solo aplica a pedidos al por mayor (20+ de un tipo).
- Pedido inmediato en horario (8am-11pm): el bot NO lo confirma de una; dice que está confirmando disponibilidad del domiciliario y el equipo responde Sí/No (o vence en 10 min -> se ofrece programar).
- Programado: mínimo 3 horas de anticipación; se confirma solo.
- Pasa a humano si: el cliente pide una persona, manda foto/audio/documento, pregunta por alianzas/emprender, envío sin tarifa o verificar transferencia. Una ubicación (pin) NO pasa a humano.
- El bot NUNCA inventa cifras: todo precio o total debe venir de una herramienta.
- Mensajes cortos, amables, en español neutro de Colombia."""


def customer_turn(persona: dict, transcript: list[dict]) -> tuple[dict, dict]:
    system = (
        f"Eres {persona['nombre']}, cliente real de Trabix Granizados escribiendo por WhatsApp. {persona['perfil']}\n"
        f"Tu objetivo: {persona['objetivo']}\n"
        "Escribe como una persona real de Armenia por WhatsApp: mensajes cortos, informales, sin sonar a IA. "
        "Solo das la información que te piden o que una persona normal daría. "
        "Pon send_location=true solo si tu perfil dice que mandas ubicación y te están pidiendo la dirección. "
        "Pon done=true cuando tu objetivo esté cumplido, sea imposible, o te digan que un asesor te escribe; en ese caso message puede ser una despedida corta."
    )
    convo = "\n".join(f"{'TÚ' if t['who'] == 'cliente' else 'TRABIX'}: {t['text']}" for t in transcript) or "(todavía no has escrito nada; escribe el primer mensaje)"
    return claude_brain.ask_json(system, f"CONVERSACIÓN HASTA AHORA:\n{convo}\n\nTu siguiente mensaje:", CUSTOMER_SCHEMA, CUSTOMER_MODEL)


def judge(persona: dict, transcript: list[dict], calls: list[dict], db: dict) -> tuple[dict, dict]:
    system = ("Eres un auditor exigente del bot de WhatsApp de Trabix Granizados. Revisa si el bot hizo lo correcto "
              "según las reglas y lo esperado del escenario. Sé concreto: cita la evidencia. No marques como problema "
              "algo que esté bien. Distingue fallas de dinero/operación (alta) de redacción (baja).\n\n" + BUSINESS_RULES)
    prompt = (
        f"ESCENARIO {persona['id']} — {persona['perfil']}\nObjetivo del cliente: {persona['objetivo']}\n"
        f"Lo esperado: {persona['esperado']}\n\n"
        "CONVERSACIÓN (lo que vio el cliente):\n" + "\n".join(f"{t['who'].upper()}: {t['text']}" for t in transcript) +
        "\n\nHERRAMIENTAS QUE LLAMÓ EL BOT Y LO QUE LE DEVOLVIERON:\n" + json.dumps(calls, ensure_ascii=False)[:20000] +
        "\n\nESTADO FINAL EN LA BASE:\n" + json.dumps(db, ensure_ascii=False, default=str)
    )
    return claude_brain.ask_json(system, prompt, JUDGE_SCHEMA, JUDGE_MODEL)


def db_snapshot(phone: str) -> dict:
    orders = sim.sql(f"""SELECT coalesce(json_agg(json_build_object('id', o.id, 'status', o.status, 'delivery_type', o.delivery_type,
        'payment', o.payment_method, 'delivery_cost', o.delivery_cost, 'total', coalesce(o.total_final, o.total_estimated),
        'referral', o.referral_code,
        'items', (SELECT json_agg(json_build_object('flavor', oi.flavor, 'qty', oi.quantity, 'unit', oi.unit_price)) FROM order_items oi WHERE oi.order_id=o.id))
        ORDER BY o.id), '[]') FROM orders o JOIN conversations c ON c.id=o.conversation_id WHERE c.phone_number='{phone}'""")
    conv = sim.sql(f"SELECT json_build_object('state', state, 'takeover', human_takeover_until > now(), 'address', delivery_address) FROM conversations WHERE phone_number='{phone}'")
    return {"orders": json.loads(orders or "[]"), "conversation": json.loads(conv or "{}")}


def wait_bot(phone: str, prev_out: int, timeout: float = 240.0) -> dict:
    """El cerebro con Claude Code tarda varios segundos por llamada: espera a que
    la bandeja de este teléfono deje de crecer 6 s seguidos."""
    end = time.time() + timeout
    last, since = prev_out, time.time()
    while time.time() < end:
        st = sim.fake_state()
        n = len([o for o in st["outbox"] if o["to"] == phone])
        if n != last:
            last, since = n, time.time()
        elif n > prev_out and time.time() - since > 6:
            return st
        time.sleep(1)
    return sim.fake_state()


def cost(usages: list[dict]) -> dict:
    tot = {k: sum(u.get(k, 0) for u in usages) for k in ("input", "cache_write", "cache_read", "output")}
    out = {"tokens": tot, "llamadas": len(usages)}
    for name, p in PRICES.items():
        usd = (tot["input"] * p["in"] + tot["output"] * p["out"] + tot["cache_read"] * p["read"] + tot["cache_write"] * p["write"]) / 1e6
        out[name] = {"usd": round(usd, 4), "cop": round(usd * COP_PER_USD)}
    return out


def run_persona(idx: int, persona: dict) -> dict:
    phone = f"5730077{idx:05d}"
    sim.reset_phone(phone)
    sim.http("POST", f"{sim.FAKE}/__reset", {})
    transcript: list[dict] = []
    side_usage: list[dict] = []
    events: list[str] = []
    for turn in range(MAX_TURNS):
        msg, u = customer_turn(persona, transcript)
        side_usage.append(u)
        if msg.get("done") and turn > 0:
            if msg.get("message"):
                transcript.append({"who": "cliente", "text": msg["message"]})
            break
        st = sim.fake_state()
        prev_out = len([o for o in st["outbox"] if o["to"] == phone])
        if msg.get("send_location") and persona.get("location_on_address"):
            transcript.append({"who": "cliente", "text": "[manda su ubicación de WhatsApp]"})
            sim.webhook(phone, persona["nombre"], {"type": "location", "location": {"latitude": 4.5571, "longitude": -75.6608}})
        else:
            transcript.append({"who": "cliente", "text": msg["message"]})
            sim.webhook(phone, persona["nombre"], {"type": "text", "text": {"body": msg["message"]}})
        st = wait_bot(phone, prev_out)
        for o in [o for o in st["outbox"] if o["to"] == phone][prev_out:]:
            transcript.append({"who": "bot", "text": o["text"]})

        state = sim.state_of(phone)
        if state == "wait_availability":
            prev_out = len([o for o in sim.fake_state()["outbox"] if o["to"] == phone])
            if persona["team"] is None:
                events.append("Nadie respondió la disponibilidad: se forzó el vencimiento.")
                sim.sql(f"""UPDATE conversations SET state_data = jsonb_set(state_data, '{{availability_deadline}}', to_jsonb(to_char((now() - interval '1 minute') AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))) WHERE phone_number='{phone}'""")
                sim.wait_for(lambda: sim.state_of(phone) != "wait_availability", timeout=90)
            else:
                events.append(f"El equipo respondió disponibilidad: {'Sí' if persona['team'] else 'No'}.")
                sim.http("POST", f"{sim.BOT_INTERNAL}/internal/orders/{sim.last_order_id(phone)}/availability",
                         {"case_phone": phone, "available": persona["team"], "sent_by": "sim"},
                         {"X-Internal-Token": sim.INTERNAL_TOKEN})
            st = wait_bot(phone, prev_out, timeout=30)
            for o in [o for o in st["outbox"] if o["to"] == phone][prev_out:]:
                transcript.append({"who": "bot", "text": o["text"]})
        if sim.takeover_active(phone) == "true":
            events.append("El caso pasó a un humano.")
    st = sim.fake_state()
    db = db_snapshot(phone)
    verdict, ju = judge(persona, transcript, st.get("brain_calls", []), db)
    return {
        "persona": persona, "transcript": transcript, "events": events, "calls": st.get("brain_calls", []),
        "brain_errors": st.get("brain_errors", []), "db": db, "verdict": verdict,
        "costo_bot": cost(st.get("brain_usage", [])), "costo_simulacion": cost(side_usage + [ju]),
    }


def report(results: list[dict]) -> str:
    icon = {"bien": "✅", "con_problemas": "⚠️", "grave": "❌"}
    lines = [f"# Suite con IA — {dt.datetime.now():%Y-%m-%d %H:%M}", "",
             f"Cerebro del bot: `{claude_brain.MODEL}` vía Claude Code · cliente: `{CUSTOMER_MODEL}` · juez: `{JUDGE_MODEL}`", ""]
    lines += ["| Escenario | Veredicto | Llamadas del bot | Costo API estimado (actual) |", "|---|---|---|---|"]
    for r in results:
        c = r["costo_bot"]["claude-sonnet-5 (actual)"]
        lines.append(f"| {r['persona']['id']} {r['persona']['nombre']} | {icon.get(r['verdict'].get('veredicto'), '?')} {r['verdict'].get('veredicto')} | {r['costo_bot']['llamadas']} | ${c['usd']} (~{c['cop']} COP) |")
    lines += ["", "## Costo de producción estimado por conversación", "",
              "Tokens que usó el bot en esta corrida, cobrados a precio de API. No es lo que pagaste: corrió con tu suscripción.", "",
              "| Modelo | Total corrida (USD) | Promedio por conversación (COP) |", "|---|---|---|"]
    for name in PRICES:
        total = sum(r["costo_bot"][name]["usd"] for r in results)
        avg = total / max(1, len(results)) * COP_PER_USD
        lines.append(f"| {name} | ${total:.3f} | ~{avg:,.0f} |")
    for r in results:
        p, v = r["persona"], r["verdict"]
        lines += ["", f"## {p['id']} — {p['nombre']}: {icon.get(v.get('veredicto'), '?')} {v.get('veredicto')}", "",
                  f"**Perfil:** {p['perfil']}  ", f"**Esperado:** {p['esperado']}", "", f"**Juez:** {v.get('resumen')}"]
        for prob in v.get("problemas", []):
            lines += [f"- **[{prob['gravedad']}] {prob['que']}** — {prob['evidencia']} → _{prob['sugerencia']}_"]
        if r["events"]:
            lines += ["", "Eventos: " + " ".join(r["events"])]
        if r["brain_errors"]:
            lines += ["", f"⚠️ Errores del cerebro simulado: {len(r['brain_errors'])}"]
        lines += ["", "<details><summary>Conversación</summary>", ""]
        lines += [f"- **{t['who']}:** {t['text']}" for t in r["transcript"]]
        lines += ["", "</details>"]
    return "\n".join(lines) + "\n"


def main() -> int:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    results = []
    for i, p in enumerate(PERSONAS, start=1):
        if ONLY and p["id"] not in ONLY:
            continue
        print(f"▶ {p['id']} {p['nombre']}…", flush=True)
        try:
            r = run_persona(i, p)
        except Exception as err:
            print(f"   falló la simulación: {err}", flush=True)
            continue
        results.append(r)
        print(f"   {r['verdict'].get('veredicto')} · {r['costo_bot']['llamadas']} llamadas del bot", flush=True)
        (OUT_DIR / "datos.json").write_text(json.dumps(results, ensure_ascii=False, indent=1, default=str))
        (OUT_DIR / "reporte.md").write_text(report(results))
    print(f"\nReporte: {OUT_DIR / 'reporte.md'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
