"""Suite de escenarios de clientes de Armenia contra el bot REAL, sin gastar
créditos de Anthropic ni mandar nada a WhatsApp.

Qué prueba: todo lo determinista que decide plata y operación — precios,
promo, domicilio por zona, domicilio gratis 6-19, pueblos y mínimos, envío
nacional, códigos de embajador, pasos a humano, ubicación, disponibilidad de
pedidos inmediatos, plazos vencidos y el estado que queda en la base.

Qué NO prueba: cómo redacta Claude ni qué herramienta elegiría. El "modelo"
es un guion fijo por escenario (fake_services.py); el bot real ejecuta las
herramientas y la suite revisa lo que el bot hizo con ellas.

Uso: ver tests/sim/README.md.
"""

from __future__ import annotations

import datetime as dt
import hashlib
import hmac
import json
import os
import subprocess
import sys
import time
import urllib.request

BOT = os.environ.get("SIM_BOT_URL", "http://127.0.0.1:8089")
BOT_INTERNAL = os.environ.get("SIM_BOT_INTERNAL_URL", "http://127.0.0.1:8081")
FAKE = os.environ.get("SIM_FAKE_URL", "http://127.0.0.1:9900")
DB = os.environ.get("SIM_DATABASE_URL", "postgresql://postgres@127.0.0.1:54329/trabix")
PSQL = os.environ.get("PSQL", "/opt/homebrew/opt/postgresql@18/bin/psql")
APP_SECRET = os.environ.get("SIM_APP_SECRET", "fake")
INTERNAL_TOKEN = os.environ.get("SIM_INTERNAL_TOKEN", "local-internal-token")
ONLY = set(sys.argv[1:])

if "railway" in DB or "rlwy" in DB:
    sys.exit("SIM_DATABASE_URL apunta a Railway: esta suite solo corre contra una base local.")


# ── utilidades ──────────────────────────────────────────────────────────────

def http(method: str, url: str, body: dict | None = None, headers: dict | None = None) -> tuple[int, dict]:
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method, headers={"content-type": "application/json", **(headers or {})})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            raw = r.read()
            return r.status, json.loads(raw) if raw else {}
    except urllib.error.HTTPError as e:
        raw = e.read()
        try:
            return e.code, json.loads(raw)
        except json.JSONDecodeError:
            return e.code, {"raw": raw.decode(errors="replace")}


def sql(query: str) -> str:
    out = subprocess.run([PSQL, DB, "-Atq", "-c", query], capture_output=True, text=True)
    if out.returncode != 0:
        raise RuntimeError(out.stderr.strip())
    return out.stdout.strip()


def fake_state() -> dict:
    return http("GET", f"{FAKE}/__state")[1]


MSG_N = {"n": 0}


def webhook(phone: str, name: str, message: dict) -> None:
    MSG_N["n"] += 1
    msg = {"from": phone, "id": f"wamid.sim.in.{int(time.time())}.{MSG_N['n']}", "timestamp": str(int(time.time())), **message}
    payload = {
        "object": "whatsapp_business_account",
        "entry": [{"id": "sim", "changes": [{"field": "messages", "value": {
            "messaging_product": "whatsapp",
            "metadata": {"display_phone_number": "573000000000", "phone_number_id": "fake"},
            "contacts": [{"profile": {"name": name}, "wa_id": phone}],
            "messages": [msg],
        }}]}],
    }
    body = json.dumps(payload).encode()
    sig = hmac.new(APP_SECRET.encode(), body, hashlib.sha256).hexdigest()
    req = urllib.request.Request(f"{BOT}/webhook", data=body, method="POST",
                                 headers={"content-type": "application/json", "X-Hub-Signature-256": f"sha256={sig}"})
    with urllib.request.urlopen(req, timeout=30) as r:
        r.read()


def settle(prev_out: int, prev_llm: int, timeout: float = 20.0) -> dict:
    """Espera a que el bot termine el turno: bandeja y llamadas quietas 1.5 s."""
    deadline = time.time() + timeout
    last = (prev_out, prev_llm)
    stable_since = time.time()
    while time.time() < deadline:
        st = fake_state()
        cur = (len(st["outbox"]), len(st["llm_calls"]))
        if cur != last:
            last, stable_since = cur, time.time()
        elif cur != (prev_out, prev_llm) and time.time() - stable_since > 1.5:
            return st
        time.sleep(0.25)
    return fake_state()


def wait_for(predicate, timeout: float, every: float = 2.0) -> bool:
    end = time.time() + timeout
    while time.time() < end:
        if predicate():
            return True
        time.sleep(every)
    return predicate()


def reset_phone(phone: str) -> None:
    sql(f"""
      DELETE FROM order_items WHERE order_id IN (SELECT o.id FROM orders o JOIN conversations c ON c.id=o.conversation_id WHERE c.phone_number='{phone}');
      DELETE FROM orders WHERE conversation_id IN (SELECT id FROM conversations WHERE phone_number='{phone}');
      DELETE FROM agent_case_messages WHERE phone_number='{phone}';
      DELETE FROM customer_addresses WHERE customer_phone_meta='{phone}';
      DELETE FROM message_events WHERE case_phone='{phone}';
      DELETE FROM conversations WHERE phone_number='{phone}';
      DELETE FROM customers WHERE phone_number_meta='{phone}';
    """)


# ── guiones reutilizables ───────────────────────────────────────────────────

def item(flavor_id: str, qty: int, wording: str, liquor: bool = True) -> list:
    return ["add_order_item", {"flavor_id": flavor_id, "has_liquor": liquor, "quantity": qty, "customer_wording": wording}]


UVA = "liquor_uva_vodka"
TOMORROW = (dt.datetime.now(dt.timezone(dt.timedelta(hours=-5))) + dt.timedelta(days=1)).strftime("%Y-%m-%d")


def immediate_order_until_availability(qty: int = 3) -> list:
    return [
        {"tools": [
            ["set_customer_field", {"field": "name", "value": "Cliente Simulado"}],
            ["set_customer_field", {"field": "phone", "value": "3001234567"}],
            item(UVA, qty, "uva vodka"),
            ["set_customer_field", {"field": "address", "value": "Calle 10 # 5-20 La Castellana"}],
            ["resolve_armenia_address", {"address": "Calle 10 # 5-20 La Castellana"}],
            ["set_delivery_immediate", {}],
        ]},
        {"tools": [["finalize_checkout", {}]]},
        {"text": "Estoy confirmando disponibilidad del domiciliario, ya te escribo."},
    ]


def state_of(phone: str) -> str:
    return sql(f"SELECT state FROM conversations WHERE phone_number='{phone}'")


def last_order_status(phone: str) -> str:
    return sql(f"SELECT o.status FROM orders o JOIN conversations c ON c.id=o.conversation_id WHERE c.phone_number='{phone}' ORDER BY o.id DESC LIMIT 1")


def last_order_id(phone: str) -> int:
    return int(sql(f"SELECT o.id FROM orders o JOIN conversations c ON c.id=o.conversation_id WHERE c.phone_number='{phone}' ORDER BY o.id DESC LIMIT 1"))


def takeover_active(phone: str) -> str:
    return sql(f"SELECT (human_takeover_until > now())::text FROM conversations WHERE phone_number='{phone}'") or "false"


HELLO = {"send": "Hola"}

# Cada escenario: teléfono, nombre y pasos. Un paso puede mandar un mensaje
# (con guion), llamar un endpoint interno, correr SQL de preparación o esperar
# el barrido; después revisa lo que salió.
SCENARIOS: list[dict] = [
    {"id": "S01", "title": "Cliente nuevo saluda: bienvenida fija, sin gastar modelo", "steps": [
        {"send": "Hola buenas", "expect": {"out": ["bienvenid"], "llm_calls": 0}},
    ]},
    {"id": "S02", "title": "6 unidades en La Castellana (norte): domicilio gratis", "steps": [
        HELLO,
        {"send": "quiero 6 de uva vodka para La Castellana", "script": [
            {"tools": [item(UVA, 6, "uva vodka"), ["resolve_armenia_address", {"address": "La Castellana"}]]},
            {"text": "Listo, te quedan con domicilio gratis."}],
         "expect": {"tools": ["GRATIS", "Total de unidades en el pedido: 6"]}},
    ]},
    {"id": "S03", "title": "Baja de 6 a 3: el domicilio vuelve a cobrarse ($6.000 norte)", "steps": [
        HELLO,
        {"send": "6 de uva para La Castellana", "script": [
            {"tools": [item(UVA, 6, "uva vodka"), ["resolve_armenia_address", {"address": "La Castellana"}]]},
            {"text": "Listo."}]},
        {"send": "mejor solo 3", "script": [
            {"tools": [["remove_order_item", {"flavor": "Uva Vodka", "has_liquor": True}], item(UVA, 3, "uva vodka")]},
            {"text": "Cambiado."}],
         "expect": {"tools": ["EL DOMICILIO CAMBIÓ", "6.000"]}},
    ]},
    {"id": "S04", "title": "Sube de 3 a 6: el domicilio pasa a gratis", "steps": [
        HELLO,
        {"send": "3 de uva al sur, La Clarita", "script": [
            {"tools": [item(UVA, 3, "uva vodka"), ["resolve_armenia_address", {"address": "La Clarita"}]]},
            {"text": "Listo."}],
         "expect": {"tools": ["10.000", "agrega 3 unidades"]}},
        {"send": "agrégale 3 más", "script": [
            {"tools": [item(UVA, 3, "uva vodka")]}, {"text": "Listo."}],
         "expect": {"tools": ["EL DOMICILIO CAMBIÓ", "GRATIS"]}},
    ]},
    {"id": "S05", "title": "20 unidades: mayorista, domicilio se cobra", "steps": [
        HELLO,
        {"send": "20 de uva para Granada", "script": [
            {"tools": [item(UVA, 20, "uva vodka"), ["resolve_armenia_address", {"address": "Granada"}]]},
            {"text": "Listo."}],
         "expect": {"tools": ["MAYORISTA", "8.000"]}},
    ]},
    {"id": "S06", "title": "Barrio de Armenia pero en Calarcá: no se cobra como Armenia", "steps": [
        HELLO,
        {"send": "6 de uva, barrio Granada, Calarcá", "script": [
            {"tools": [item(UVA, 6, "uva vodka"), ["resolve_armenia_address", {"address": "Barrio Granada, Calarcá"}]]},
            {"tools": [["set_delivery_nearby_town", {"town": "Calarcá"}]]},
            {"text": "Para Calarcá el domicilio se cobra."}],
         "expect": {"tools": ["NO es Armenia", "Calarcá: domicilio $15.000"]}},
    ]},
    {"id": "S07", "title": "'Quimbaya' sin decir Armenia: el bot pregunta barrio o municipio", "steps": [
        HELLO,
        {"send": "3 de uva para la calle 10 quimbaya", "script": [
            {"tools": [item(UVA, 3, "uva vodka"), ["resolve_armenia_address", {"address": "calle 10 quimbaya"}]]},
            {"text": "¿Es el barrio Quimbaya de Armenia o el municipio?"}],
         "expect": {"tools": ["a la vez un barrio de Armenia y un municipio"]}},
    ]},
    {"id": "S08", "title": "Salento con 5 unidades: mínimo 20", "steps": [
        HELLO,
        {"send": "5 de uva para Salento", "script": [
            {"tools": [item(UVA, 5, "uva vodka"), ["set_delivery_nearby_town", {"town": "Salento"}]]},
            {"text": "Para Salento el mínimo es mayor."}],
         "expect": {"tools": ["mínimo es de 20"]}},
    ]},
    {"id": "S09", "title": "Promo: 2 con licor $12.000; 2 sin licor $14.000 (sin promo)", "steps": [
        HELLO,
        {"send": "2 de uva y 2 blueberry sin licor", "script": [
            {"tools": [item(UVA, 2, "uva vodka")]},
            {"tools": [["remove_order_item", {"flavor": "Uva Vodka", "has_liquor": True}], item("non_liquor_blueberry", 2, "blueberry sin licor", liquor=False)]},
            {"text": "Listo."}],
         "expect": {"tools": ["12.000", "14.000"]}},
    ]},
    {"id": "S10", "title": "Código de embajador en un pedido al detal: no aplica", "steps": [
        HELLO,
        {"send": "3 de uva, tengo el código jega1", "script": [
            {"tools": [item(UVA, 3, "uva vodka"), ["apply_referral_code", {"code": "jega1"}]]},
            {"text": "Ese código solo aplica al por mayor."}],
         "expect": {"tools": ["no tiene productos al por mayor"]}},
    ]},
    {"id": "S11", "title": "Manda su ubicación (pin): el bot sigue, sin pasar a humano", "steps": [
        HELLO,
        {"send": {"type": "location", "location": {"latitude": 4.5161168, "longitude": -75.6998982}},
         "script": [{"text": "Gracias por la ubicación. ¿En qué barrio y dirección es?"}],
         "expect": {"out": ["En qué barrio"], "sql": [("takeover", "false")], "llm_calls_min": 1}},
    ]},
    {"id": "S12", "title": "Manda una foto: pasa a humano sin llamar al modelo", "steps": [
        HELLO,
        {"send": {"type": "image", "image": {"id": "media.sim.img1", "mime_type": "image/jpeg"}},
         "expect": {"llm_calls": 0, "sql": [("takeover", "true")]}},
    ]},
    {"id": "S13", "title": "Pide hablar con una persona: pasa a humano", "steps": [
        HELLO,
        {"send": "quiero hablar con una persona", "expect": {"llm_calls": 0, "sql": [("takeover", "true")]}},
    ]},
    {"id": "S14", "title": "Pedido a Bogotá (nacional) 20 u: llega descongelado y pasa a humano para cotizar", "steps": [
        HELLO,
        {"send": "20 de uva para Bogotá", "script": [
            {"tools": [
                ["set_customer_field", {"field": "name", "value": "Cliente Bogotá"}],
                ["set_customer_field", {"field": "phone", "value": "3001234567"}],
                item(UVA, 20, "uva vodka"),
                ["set_customer_field", {"field": "address", "value": "Calle 100 # 15-20 Bogotá"}],
                ["set_delivery_national", {"city": "Bogotá"}],
                ["skip_referral_code", {}],
                ["set_delivery_immediate", {}]]},
            {"tools": [["finalize_checkout", {}]]},
            {"text": "Un asesor te confirma el valor del envío."}],
         "expect": {"tools": ["DESCONGELADO"], "sql": [("takeover", "true")]}},
    ]},
    {"id": "S15", "title": "Inmediato en horario: espera disponibilidad → 'Sí, lo despacho'", "needs_open_hours": True, "steps": [
        HELLO,
        {"send": "quiero 3 de uva ya para Calle 10 # 5-20 La Castellana", "script": immediate_order_until_availability(),
         "expect": {"tools": ["confirmando", "disponibilidad"], "sql": [("state", "wait_availability"), ("order_status", "awaiting_courier")]}},
        {"availability": True, "expect": {"out": ["Confirmado"], "sql": [("state", "select_payment_method"), ("order_status", "draft_payment")]}},
        {"send": "contra entrega", "script": [
            {"tools": [["set_payment_method", {"method": "cash_on_delivery"}]]}, {"text": "¡Listo!"}],
         "expect": {"sql": [("order_status", "confirmed")]}},
    ]},
    {"id": "S16", "title": "Inmediato en horario → 'No puedo': se cancela y se ofrece programar", "needs_open_hours": True, "steps": [
        HELLO,
        {"send": "3 de uva ya", "script": immediate_order_until_availability(),
         "expect": {"sql": [("state", "wait_availability")]}},
        {"availability": False, "expect": {"out": ["no tenemos domiciliario", "programo"], "sql": [("state", "main_menu"), ("order_status", "cancelled")]}},
        {"send": "mañana a las 3 pm", "script": [
            {"tools": [["set_delivery_schedule", {"date": TOMORROW, "time": "15:00"}]]},
            {"tools": [["finalize_checkout", {}]]},
            {"text": "Listo, quedó programado."}],
         "expect": {"tools": ["auto-aceptado"], "sql": [("state", "select_payment_method")]}},
    ]},
    {"id": "S17", "title": "Inmediato: nadie responde y vence el plazo", "needs_open_hours": True, "steps": [
        HELLO,
        {"send": "3 de uva ya", "script": immediate_order_until_availability(),
         "expect": {"sql": [("state", "wait_availability")]}},
        {"sql_setup": """UPDATE conversations SET state_data = jsonb_set(state_data, '{availability_deadline}', to_jsonb(to_char((now() - interval '1 minute') AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))) WHERE phone_number='{phone}'""",
         "wait_sweep": "main_menu",
         "expect": {"out": ["No alcanzamos"], "sql": [("order_status", "cancelled")]}},
    ]},
    {"id": "S18", "title": "Foto y nadie atiende en 15 min: el bot avisa y retoma", "steps": [
        HELLO,
        {"send": {"type": "image", "image": {"id": "media.sim.img2", "mime_type": "image/jpeg"}},
         "expect": {"sql": [("takeover", "true")]}},
        {"sql_setup": "UPDATE message_events SET created_at = created_at - interval '20 minutes' WHERE case_phone='{phone}'",
         "wait_sweep": "takeover_off",
         "expect": {"out": ["no hay un asesor disponible"], "sql": [("takeover", "false")]}},
    ]},
]


def check_sql(phone: str, key: str) -> str:
    return {
        "state": lambda: state_of(phone),
        "order_status": lambda: last_order_status(phone),
        "takeover": lambda: takeover_active(phone),
    }[key]()


def is_open_hours() -> bool:
    h = dt.datetime.now(dt.timezone(dt.timedelta(hours=-5))).hour
    return 8 <= h < 23


def run() -> int:
    passed = failed = skipped = 0
    for i, sc in enumerate(SCENARIOS, start=1):
        if ONLY and sc["id"] not in ONLY:
            continue
        if sc.get("needs_open_hours") and not is_open_hours():
            print(f"⏭  {sc['id']} {sc['title']} (fuera de horario 8am-11pm, se omite)")
            skipped += 1
            continue
        phone = f"5730099{i:05d}"
        reset_phone(phone)
        http("POST", f"{FAKE}/__reset", {})
        problems: list[str] = []
        for n, step in enumerate(sc["steps"], start=1):
            st = fake_state()
            prev_out, prev_llm = len(st["outbox"]), len(st["llm_calls"])
            http("POST", f"{FAKE}/__script", {"responses": step.get("script", [])})

            if "send" in step:
                msg = step["send"]
                webhook(phone, "Cliente Simulado", {"type": "text", "text": {"body": msg}} if isinstance(msg, str) else msg)
                st = settle(prev_out, prev_llm)
            elif "availability" in step:
                code, body = http("POST", f"{BOT_INTERNAL}/internal/orders/{last_order_id(phone)}/availability",
                                  {"case_phone": phone, "available": step["availability"], "sent_by": "sim"},
                                  {"X-Internal-Token": INTERNAL_TOKEN})
                if code != 200:
                    problems.append(f"paso {n}: availability respondió {code} {body}")
                st = settle(prev_out, prev_llm)
            elif "sql_setup" in step:
                sql(step["sql_setup"].replace("{phone}", phone))
                target = step["wait_sweep"]
                ok = wait_for((lambda: state_of(phone) == "main_menu") if target == "main_menu"
                              else (lambda: takeover_active(phone) == "false"), timeout=90)
                if not ok:
                    problems.append(f"paso {n}: el barrido no actuó en 90 s")
                st = settle(prev_out, prev_llm, timeout=5)

            new_out = " || ".join(o["text"] for o in st["outbox"][prev_out:])
            new_calls = st["llm_calls"][prev_llm:]
            tool_text = " || ".join(t for c in new_calls for t in c["tool_results"])
            exp = step.get("expect", {})
            for s in exp.get("out", []):
                if s.lower() not in new_out.lower():
                    problems.append(f"paso {n}: el cliente no recibió «{s}». Recibió: {new_out[:300] or '(nada)'}")
            for s in exp.get("tools", []):
                if s.lower() not in tool_text.lower():
                    problems.append(f"paso {n}: ninguna herramienta devolvió «{s}». Devolvieron: {tool_text[:400] or '(nada)'}")
            if "llm_calls" in exp and len(new_calls) != exp["llm_calls"]:
                problems.append(f"paso {n}: se esperaban {exp['llm_calls']} llamadas al modelo y hubo {len(new_calls)}")
            if "llm_calls_min" in exp and len(new_calls) < exp["llm_calls_min"]:
                problems.append(f"paso {n}: el modelo no fue consultado")
            for key, expected in exp.get("sql", []):
                got = check_sql(phone, key)
                if got != expected:
                    problems.append(f"paso {n}: {key} = «{got}», se esperaba «{expected}»")
            if st.get("script"):
                problems.append(f"paso {n}: sobró guion sin usar ({len(st['script'])} pasos): el bot cortó antes")
        if problems:
            failed += 1
            print(f"❌ {sc['id']} {sc['title']}")
            for p in problems:
                print(f"     - {p}")
        else:
            passed += 1
            print(f"✅ {sc['id']} {sc['title']}")
    print(f"\n{passed} pasaron · {failed} fallaron · {skipped} omitidos")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(run())
