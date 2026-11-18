# Suite de escenarios sin tokens (`tests/sim/`)

Corre el bot **real** contra una base local, con un Meta falso y un Claude falso
que responde con un guion fijo. No gasta créditos de Anthropic ni manda nada a
WhatsApp.

**Prueba** lo que decide plata y operación: precios y promo, domicilio por zona,
domicilio gratis 6–19, pueblos y mínimos, envío nacional, códigos de embajador,
pasos a humano, ubicación, disponibilidad de inmediatos, plazos vencidos y el
estado que queda en la base. **No prueba** cómo redacta Claude ni qué
herramienta elegiría: eso solo se ve contra la API real.

## Correrla

```bash
# 1. Postgres local desechable (una vez)
initdb -D /tmp/trabix-sim-pg -U postgres --auth=trust
pg_ctl -D /tmp/trabix-sim-pg -o "-p 54329 -k '' -h 127.0.0.1" start
psql -h 127.0.0.1 -p 54329 -U postgres -c "create database trabix"

# 2. Servicios falsos
python3 tests/sim/fake_services.py 9900 &

# 3. Bot apuntando a los falsos (desde una carpeta SIN .env, con config/ copiada)
DATABASE_URL=postgresql://postgres@127.0.0.1:54329/trabix ADVISOR_PHONE=570000000000 \
WHATSAPP_TOKEN=fake WHATSAPP_PHONE_ID=fake WHATSAPP_VERIFY_TOKEN=fake WHATSAPP_APP_SECRET=fake \
MENU_IMAGE_MEDIA_ID=fake ANTHROPIC_API_KEY=sk-fake \
WHATSAPP_API_BASE=http://127.0.0.1:9900 ANTHROPIC_API_URL=http://127.0.0.1:9900/v1/messages \
INTERNAL_API_TOKEN=local-internal-token PORT=8089 BIND_IP=127.0.0.1 ./granizado-bot &

# 4. Escenarios (todos, o algunos por id)
python3 tests/sim/run_scenarios.py
python3 tests/sim/run_scenarios.py S15 S16
```

El bot carga `.env` con `dotenvy`: correrlo desde la raíz del repo mezclaría las
credenciales reales. El corredor se niega a correr si `SIM_DATABASE_URL` apunta
a Railway. Los escenarios de disponibilidad (S15–S17) se omiten fuera de
8 am–11 pm. S17 y S18 esperan el barrido de 60 s.

## Agregar un escenario

Cada escenario es una lista de pasos en `SCENARIOS`. Un paso manda un mensaje
con el guion de lo que "responde el modelo", o llama un endpoint interno, o
prepara la base y espera el barrido; después revisa lo que le llegó al cliente
(`out`), lo que devolvieron las herramientas (`tools`), cuántas veces se llamó
al modelo y el estado en la base (`sql`).
