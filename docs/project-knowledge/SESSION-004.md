# SESSION-004: FASE 1 - Preparación CRM y Analytics DB (2026-07-13)

## Resumen Ejecutivo

Se completó la **FASE 1 de la refactorización del bot con agente IA** estableciendo la infraestructura de base de datos para persistencia permanente de clientes y análisis de códigos de referral. Esto habilita al bot para recordar el historial completo de cada cliente y generar reportes de desempeño por embajador.

**Impacto comercial:** Con estos cambios, Trabix puede construir un CRM completo donde cada cliente (identificado por su número de WhatsApp de Meta) tiene un registro permanente de compras acumuladas, preferencias de entrega, y historial de códigos usados. Los embajadores obtienen visibilidad en tiempo real de qué códigos generan más ventas y comisiones.

---

## Qué Se Hizo

### 1. Tabla de Clientes Persistentes (`customers`)
- **Archivo:** migrations/008_create_customers_table.sql
- **Función:** Registro único por cliente basado en `phone_number_meta` (extraído de Meta, nunca cambia)
- **Campos clave:**
  - `phone_number_meta`: identificador único del cliente
  - `total_spent_cop`: dinero total gastado en pedidos confirmados
  - `total_units_purchased`: unidades totales compradas
  - `first_contact_at`, `last_contact_at`: ventanas temporales para analytics
  - Campos opcionales: nombre manual, teléfono manual, username de WhatsApp, última dirección usada
- **Índices:** búsqueda rápida por teléfono y por fecha de último contacto (para reportes recientes)

### 2. Tabla de Analytics por Código de Referral (`referral_code_analytics`)
- **Archivo:** migrations/009_create_referral_analytics.sql
- **Función:** Métricas agregadas por código embajador
- **Campos:**
  - `times_used`: cuántas veces se aplicó este código en un pedido confirmado
  - `total_discount_generated_cop`: dinero total descontado a clientes con este código
  - `total_commission_generated_cop`: comisiones totales generadas para el embajador
  - `total_units_purchased`: unidades totales bajo este código
  - `total_sales_cop`: ingresos brutos sin descuentos
- **Índices:** búsqueda por código y por fecha de actualización (reportes ordenados por recencia)

### 3. Funciones de Base de Datos Agregadas
Todas las funciones usan `ON CONFLICT... DO UPDATE` para ser idempotentes (se pueden ejecutar múltiples veces sin romper):

| Función | Propósito |
|---------|-----------|
| `get_customer()` | Buscar cliente existente por phone_number_meta |
| `create_or_update_customer()` | Crear cliente nuevo o actualizar datos existentes (upsert automático) |
| `update_customer_totals()` | Incrementar spend/units cuando confirma pedido |
| `get_referral_code_analytics()` | Buscar analytics de un código embajador |
| `create_or_update_referral_analytics()` | Crear o incrementar métricas de código (upsert) |

---

## Cambios de Código

### Archivos Modificados
1. **src/db/models.rs** (+35 líneas)
   - Agregados tipos `Customer` y `ReferralCodeAnalytics` con deserialización automática desde PostgreSQL

2. **src/db/queries.rs** (+150 líneas)
   - Agregadas 5 funciones CRUD async para manejar las nuevas tablas

3. **CHANGELOG.md** (actualizado)
   - Documentadas las nuevas funciones en sección `[Unreleased] → Added`

### Migraciones Agregadas
- **migrations/008_create_customers_table.sql**: definición de tabla `customers` con 12 campos, 2 índices
- **migrations/009_create_referral_analytics.sql**: definición de tabla `referral_code_analytics` con 7 campos, 2 índices

### Compilación
- ✅ `cargo check` pasó sin errores
- ✅ Todos los tipos compilan correctamente
- ✅ Migraciones numeradas y en orden

---

## Qué Habilita Esta Fase

1. **CRM completo:** El bot ahora puede recordar a cada cliente para siempre sin limpiar el historial tras cada pedido
2. **Reportes de cliente:** consultas futuras pueden mostrar "este cliente ha gastado $500k en 200 unidades"
3. **Analytics de embajadores:** saber exactamente cuántos pesos genera cada código de descuento
4. **Comisiones automáticas:** calcular lo que Trabix debe pagar a cada embajador basado en `total_commission_generated_cop`

---

## Próximo Paso: FASE 2

**FASE 2 (Tools Deterministas)** — próxima sesión
- Crear 3 funciones en `src/ai/tools.rs` que el agente invocará:
  1. `calculate_order_with_delivery()` — calcula total incluyendo domicilio + referral
  2. `get_delivery_cost()` — busca costo automático por zona/pueblo
  3. `apply_referral_discount()` — aplica descuento y calcula comisión
- Estas funciones reemplazan lógica duplicada que vive hoy en múltiples lugares

---

## Riesgos Identificados

| Riesgo | Mitigación |
|--------|-----------|
| **Migraciones en prod sin backup** | Always backup PostgreSQL on Railway antes de aplicar migraciones nuevas |
| **ON CONFLICT puede enmascarar errores** | Validar primero que datos sean correctos antes de update, no después |
| **Historial infinito de clientes** | Implementar políticas de retention después (no aplica ahora) |

---

## Estado Actual

- ✅ Migraciones 008–009 creadas y verificadas
- ✅ Modelos compilando correctamente
- ✅ Queries CRUD funcionales (no testeadas en prod aún)
- ✅ Commit: `5acfc9d` en master
- ⏳ Siguiente: FASE 2 (tools deterministas)

---

**Sesión finalizada:** 2026-07-13 09:45 UTC-5  
**Tiempo dedicado:** ~30 min  
**Archivos afectados:** 5 (modelos, queries, 2 migraciones, changelog)  
**Líneas de código:** +193 insertions
