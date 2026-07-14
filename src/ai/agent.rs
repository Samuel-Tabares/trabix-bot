//! Loop de agente con tool-calling. Cubre todo el caso de un pedido: desde
//! el saludo con el cliente hasta el puente con el asesor humano (domicilio,
//! disponibilidad, pago, comprobante). El asesor y el cliente comparten el
//! mismo "cerebro" por caso (misma transcripcion en `agent_case_messages`,
//! misma `ConversationContext`) — el LLM decide a quien hablarle en cada
//! momento usando `message_customer`/`message_advisor` explicitamente.
//!
//! Guardrail de diseno (no negociable): el modelo nunca calcula precios ni
//! decide reglas de negocio a mano. Cada tool delega en `src/ai/tools.rs`,
//! `src/bot/pricing.rs`, `src/bot/delivery_zone.rs` o `src/referrals.rs`,
//! que son deterministicos y estan probados. El modelo solo elige que tool
//! llamar, con que argumentos, y como redactar el mensaje alrededor del
//! resultado.
//!
//! Fuera de alcance de esta version (siguen deterministicos, sin romper):
//! renegociacion de hora cuando el asesor no puede atender de inmediato,
//! el flujo de "tomar pedido al por mayor", y "Hablar con Asesor" sin
//! pedido. Los timers (recordatorios, vencimientos) tampoco invocan al LLM:
//! siguen disparando los mismos mensajes genericos de siempre para ahorrar
//! llamadas al modelo; el LLM solo entra cuando hace falta razonar algo.

use std::error::Error;

use serde_json::{json, Value};

use crate::{
    ai::{
        client::{AnthropicClient, ContentBlock, Message, ToolDefinition},
        memory, tools,
    },
    bot::{
        delivery_zone::{self, ArmeniaZone, MIN_UNITS_OUTSIDE_ARMENIA},
        state_machine::{BotAction, ConversationContext, ConversationState, ImageAsset, TimerType, UserInput},
        states::checkout,
        timers::{ADVISOR_RESPONSE_TIMEOUT, RECEIPT_TIMEOUT},
    },
    db::models::OrderItemData,
    AppState,
};

const MAX_TOOL_ITERATIONS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    Customer,
    Advisor,
}

const SYSTEM_PROMPT: &str = r#"Eres quien atiende WhatsApp para Trabix Granizados, una marca de \
granizados sellados listos para consumir (con licor 12% y sin licor) en Armenia, Quindio, \
Colombia. Hablas como una persona real del negocio: calida, breve, en espanol colombiano, sin \
acento, texto plano, usa emojis, se directo, sin sonar a formulario.

Eres el puente completo entre el cliente y el asesor humano: hablas con ambos. Cada uno tiene su \
propio numero de WhatsApp. Cuando le respondas directamente a quien te acaba de escribir en este \
turno, simplemente escribe tu respuesta como texto normal (no necesitas ninguna herramienta para \
eso, se envia automaticamente a esa persona). Usa message_customer o message_advisor solo cuando \
en el mismo turno necesites decirle algo a la OTRA persona (por ejemplo: el cliente te escribe y \
tu, ademas de responderle, necesitas avisarle algo al asesor).

CUÁNDO USAR message_advisor (solo en estos casos):
1. El cliente solicita hablar con asesor o necesita atención especial → avísale al asesor quién es, \
   qué número tiene, y cuál es la consulta.
2. Confirmar disponibilidad de pedido inmediato → pregunta "¿Puedes entregar ahorita?" con resumen.
3. Domicilio en municipio desconocido → pide el costo: "¿Cuál es el domicilio a [municipio]?".
4. Casos fuera de tema o que requieren criterio comercial → redirige al asesor con contexto claro.

INTERACCIÓN BOTONES VS. TEXTO LIBRE:
- Si el cliente presiona un botón (Hacer Pedido, Ver Menú): respuesta determinista, sin comentario extra.
- Si el cliente escribe texto libre: analiza, ejecuta tools necesarias, responde con flexibilidad.

REGLA MAYORISTA + REFERRAL:
- Un pedido es mayorista si tiene 20+ unidades del MISMO tipo (con o sin licor).
- Si es mayorista Y el domicilio ya es conocido (zona Armenia o pueblo cercano), ANTES de pedir \
  método de pago pregunta: "¿Tienes un código de referido o descuento?"
- Si dice sí: valida con apply_referral_code. Si es válido: muestra descuento y recalcula total. \
  Si es inválido: ofrece reintentar o seguir sin código.

DOMICILIO AUTOMÁTICO (no pidas al asesor si puedes resolverlo):
- Armenia: pregunta zona (norte/centro/sur) → set_delivery_zone_armenia → costo automático ✓
- Pueblo cercano conocido (Bogotá, etc.): lookup_nearby_town → si existe y ≥20 unidades → \
  set_delivery_nearby_town → costo automático ✓
- Pueblo cercano pero <20 unidades: rechaza con "Mínimo 20 unidades para ese destino" ✓
- Municipio desconocido: message_advisor pidiendo costo → set_manual_delivery_cost ✓

Reglas que no puedes romper:
- Cuando alguien te da varios datos en un solo mensaje (nombre, teléfono, dirección, sabor, \
  cantidad, tipo de entrega, zona, etc.), guarda TODOS esos datos en el mismo turno llamando cada \
  herramienta correspondiente (set_customer_field varias veces si hace falta, add_order_item, \
  set_delivery_immediate/set_delivery_schedule, set_delivery_zone_armenia, etc.) antes de \
  responder. No dejes para después un dato que ya te dieron, y no vuelvas a preguntar por algo que \
  el bloque "ESTADO ACTUAL DEL CASO" ya muestra como conocido.
- Nunca inventes precios, sabores, horarios, zonas ni disponibilidad: siempre usa una herramienta \
  para obtener esos datos antes de afirmarlos.
- El horario de entrega inmediata es el que te diga check_business_hours, nunca asumas uno.
- Antes de agregar un producto usa get_menu para conocer los flavor_id validos; no inventes ids.
- Antes de borrar el pedido con restart_order o cancelarlo con cancel_order, confirma \
  explicitamente con el cliente.
- Solo llama finalize_checkout cuando el cliente ya confirmo que quiere enviar el pedido tal cual \
  esta, con productos, nombre, telefono, direccion y tipo de entrega completos. En ese mismo turno, \
  ademas de llamar finalize_checkout, escribele al cliente confirmandole que el pedido fue enviado, \
  y usa message_advisor para contactar al asesor — no dejes ese aviso para un turno futuro.
- Despues de finalize_checkout, si el domicilio ya se conoce (zona o pueblo cercano), solo \
  pregunta al asesor con message_advisor si puede enviar el pedido (no le pidas el precio, ya lo \
  sabes). Si el domicilio no se conoce todavia (municipio fuera de la lista), pidele el valor.
- Cuando el asesor te responda que si puede, usa confirm_advisor_availability con available=true. \
  Si te dice que no puede, usa confirm_advisor_availability con available=false.
- Cuando el cliente elija metodo de pago, usa set_payment_method. Si elige pago por transferencia, \
  las instrucciones de transferencia las envia automaticamente esa herramienta.
- No prometas nada que no puedas confirmar con una herramienta.
- Si alguien pregunta algo fuera de estos temas, redirige con amabilidad hacia el pedido.
"#;

pub async fn run_customer_turn(
    state: &AppState,
    context: &mut ConversationContext,
    current_state: &ConversationState,
    input: &UserInput,
) -> Result<(ConversationState, Vec<BotAction>), Box<dyn Error + Send + Sync>> {
    run_case_turn(state, context, current_state, Actor::Customer, input).await
}

pub async fn run_advisor_turn(
    state: &AppState,
    context: &mut ConversationContext,
    current_state: &ConversationState,
    input: &UserInput,
) -> Result<(ConversationState, Vec<BotAction>), Box<dyn Error + Send + Sync>> {
    run_case_turn(state, context, current_state, Actor::Advisor, input).await
}

async fn run_case_turn(
    state: &AppState,
    context: &mut ConversationContext,
    current_state: &ConversationState,
    actor: Actor,
    input: &UserInput,
) -> Result<(ConversationState, Vec<BotAction>), Box<dyn Error + Send + Sync>> {
    if let Some((next_state, actions)) =
        try_handle_receipt_shortcut(context, current_state, actor, input)
    {
        return Ok((next_state, actions));
    }

    let api_key = state
        .config
        .anthropic_api_key
        .clone()
        .ok_or("ANTHROPIC_API_KEY not configured for agent engine")?;
    let client = AnthropicClient::new(api_key);
    let phone = context.phone_number.clone();

    let mut history = memory::load_messages(&state.pool, &phone).await?;
    history.push(Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text: format_inbound_message(actor, input),
        }],
    });

    let system_prompt = build_system_prompt(context, actor);
    let tool_defs = tool_definitions();

    let mut actions: Vec<BotAction> = Vec::new();
    let mut terminal: Option<(ConversationState, Vec<BotAction>)> = None;
    // Al primer tool que cambia de estado (finalize_checkout,
    // confirm_advisor_availability, set_payment_method, cancel_order) le
    // damos exactamente una ronda extra para que el modelo cierre con un
    // mensaje (p. ej. avisarle a la otra parte). Mas rondas despues de eso
    // solo dan pie a que el modelo siga hablando sobre un caso que ya
    // termino, con riesgo real de inventar totales que ya no lee de una
    // tool (como paso en pruebas: alucino un total incorrecto en una
    // ronda de mas).
    let mut terminal_bonus_round_used = false;

    for _ in 0..MAX_TOOL_ITERATIONS {
        if terminal.is_some() && terminal_bonus_round_used {
            break;
        }
        if terminal.is_some() {
            terminal_bonus_round_used = true;
        }

        let response = client
            .send_message(&system_prompt, &history, &tool_defs)
            .await?;

        history.push(Message {
            role: "assistant".to_string(),
            content: response.content.clone(),
        });

        // Texto plano (sin tool call) va para quien esta escribiendo en
        // este turno: es la respuesta natural a lo que acaban de decir.
        // message_customer/message_advisor siguen existiendo para cuando el
        // modelo necesita cruzar de audiencia (p. ej. el cliente escribe y
        // hay que avisarle algo al asesor en el mismo turno).
        for block in &response.content {
            if let ContentBlock::Text { text } = block {
                if !text.trim().is_empty() {
                    let to = match actor {
                        Actor::Customer => context.phone_number.clone(),
                        Actor::Advisor => context.advisor_phone.clone(),
                    };
                    actions.push(BotAction::SendText {
                        to,
                        body: text.clone(),
                    });
                }
            }
        }

        let tool_uses: Vec<(String, String, Value)> = response
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();

        if tool_uses.is_empty() {
            break;
        }

        let mut tool_results = Vec::with_capacity(tool_uses.len());
        for (id, name, tool_input) in tool_uses {
            tracing::info!(
                phone = %crate::logging::mask_phone(&context.phone_number),
                actor = ?actor,
                tool = %name,
                input = %tool_input,
                "agent tool call"
            );
            match dispatch_tool(&id, &name, &tool_input, context, actor, current_state) {
                ToolOutcome::Result(block) => tool_results.push(block),
                ToolOutcome::ResultWithAction(block, action) => {
                    tool_results.push(block);
                    actions.push(action);
                }
                ToolOutcome::ResultWithMenuImage(block) => {
                    tool_results.push(block);
                    actions.push(BotAction::SendAssetImage {
                        to: context.phone_number.clone(),
                        asset: ImageAsset::Menu,
                        caption: None,
                    });
                }
                ToolOutcome::ResultWithStateChange(block, next_state, mut handoff_actions) => {
                    tool_results.push(block);
                    actions.append(&mut handoff_actions);
                    // No cortamos el loop aqui: el modelo suele encadenar
                    // esto con un message_advisor/message_customer en el
                    // mismo turno (p. ej. finalize_checkout + "le pregunto
                    // al asesor si puede enviarlo"). Si cortaramos ya,
                    // ese mensaje nunca saldria. El loop igual termina solo
                    // cuando el modelo deja de pedir tools o al llegar a
                    // MAX_TOOL_ITERATIONS.
                    terminal = Some((next_state, Vec::new()));
                }
            }
        }

        history.push(Message {
            role: "user".to_string(),
            content: tool_results,
        });
    }

    memory::save_messages(&state.pool, &phone, &history).await?;

    let final_state = terminal
        .map(|(next_state, _)| next_state)
        .unwrap_or_else(|| current_state.clone());

    if actions.is_empty() {
        actions.push(BotAction::SendText {
            to: phone.clone(),
            body: "Dame un segundo y ya te ayudo.".to_string(),
        });
    }

    Ok((final_state, actions))
}

/// Si llega una imagen del cliente mientras se espera comprobante de
/// transferencia, se procesa 100% deterministico (sin llamar al LLM): es un
/// paso mecanico, no hace falta razonamiento y asi se ahorra la llamada.
fn try_handle_receipt_shortcut(
    context: &mut ConversationContext,
    current_state: &ConversationState,
    actor: Actor,
    input: &UserInput,
) -> Option<(ConversationState, Vec<BotAction>)> {
    if actor != Actor::Customer || *current_state != ConversationState::WaitReceipt {
        return None;
    }
    let UserInput::ImageMessage(media_id) = input else {
        return None;
    };

    context.receipt_media_id = Some(media_id.clone());
    context.receipt_timer_started_at = None;
    context.receipt_timer_expired = false;

    let summary = advisor_case_summary(context);
    let actions = vec![
        BotAction::CancelTimer {
            timer_type: TimerType::ReceiptUpload,
            phone: context.phone_number.clone(),
        },
        BotAction::UpsertDraftOrder {
            status: "confirmed".to_string(),
        },
        BotAction::SendText {
            to: context.advisor_phone.clone(),
            body: format!("Pedido confirmado (pago por transferencia):\n\n{summary}"),
        },
        BotAction::SendImage {
            to: context.advisor_phone.clone(),
            media_id: media_id.clone(),
            caption: Some("Comprobante".to_string()),
        },
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: "¡Comprobante recibido! Tu pedido quedó confirmado 🎉".to_string(),
        },
        BotAction::ResetConversation {
            phone: context.phone_number.clone(),
        },
    ];

    Some((ConversationState::MainMenu, actions))
}

fn format_inbound_message(actor: Actor, input: &UserInput) -> String {
    let who = match actor {
        Actor::Customer => "CLIENTE",
        Actor::Advisor => "ASESOR",
    };
    let body = match input {
        UserInput::TextMessage(text) => text.clone(),
        UserInput::ButtonPress(id) | UserInput::ListSelection(id) => {
            format!("[seleccionó: {id}]")
        }
        UserInput::ImageMessage(media_id) => format!("[envió una imagen: {media_id}]"),
    };
    format!("Mensaje del {who}: {body}")
}

fn build_system_prompt(context: &ConversationContext, actor: Actor) -> String {
    let items_summary = if context.items.is_empty() {
        "vacío".to_string()
    } else {
        context
            .items
            .iter()
            .map(|item| {
                format!(
                    "{} x {} ({})",
                    item.quantity,
                    item.flavor,
                    if item.has_liquor { "con licor" } else { "sin licor" }
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    let actor_label = match actor {
        Actor::Customer => "el CLIENTE",
        Actor::Advisor => "el ASESOR humano",
    };

    format!(
        "{SYSTEM_PROMPT}\n\n---\nESTADO ACTUAL DEL CASO (dato de verdad, ignora lo que la \
        conversación sugiera si contradice esto):\nQuién te escribe en este turno: {actor_label}\n\
        Número del asesor humano: {}\nCliente conocido: nombre={:?}, teléfono={:?}, dirección={:?}\n\
        Pedido actual: {items_summary}\nTipo de entrega: {:?} (fecha={:?}, hora={:?})\n\
        Costo de domicilio ya definido: {:?}\nMétodo de pago: {:?}\nComprobante recibido: {}\n\
        Timer de espera del asesor vencido: {}\n---",
        context.advisor_phone,
        context.customer_name,
        context.customer_phone,
        context.delivery_address,
        context.delivery_type,
        context.scheduled_date,
        context.scheduled_time,
        context.delivery_cost,
        context.payment_method,
        context.receipt_media_id.is_some(),
        context.advisor_timer_expired,
    )
}

fn ok_result(tool_use_id: &str, content: impl Into<String>) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: tool_use_id.to_string(),
        content: content.into(),
        is_error: None,
    }
}

fn error_result(tool_use_id: &str, content: impl Into<String>) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: tool_use_id.to_string(),
        content: content.into(),
        is_error: Some(true),
    }
}

enum ToolOutcome {
    Result(ContentBlock),
    ResultWithAction(ContentBlock, BotAction),
    ResultWithMenuImage(ContentBlock),
    ResultWithStateChange(ContentBlock, ConversationState, Vec<BotAction>),
}

fn dispatch_tool(
    id: &str,
    name: &str,
    input: &Value,
    context: &mut ConversationContext,
    actor: Actor,
    current_state: &ConversationState,
) -> ToolOutcome {
    match name {
        "get_menu" => {
            let menu = tools::get_menu();
            ToolOutcome::Result(ok_result(
                id,
                json!({
                    "menu_text": menu.menu_text,
                    "flavors_with_liquor": menu.flavors_with_liquor,
                    "flavors_without_liquor": menu.flavors_without_liquor,
                })
                .to_string(),
            ))
        }
        "check_business_hours" => {
            let status = tools::check_business_hours();
            ToolOutcome::Result(ok_result(
                id,
                json!({ "is_open": status.is_open, "hours_text": status.hours_text }).to_string(),
            ))
        }
        "show_menu_image" => ToolOutcome::ResultWithMenuImage(ok_result(id, "Imagen enviada.")),
        "set_customer_field" => wrap(id, set_customer_field(input, context)),
        "set_delivery_immediate" => wrap(id, set_delivery_immediate(context)),
        "set_delivery_schedule" => wrap(id, set_delivery_schedule(input, context)),
        "add_order_item" => wrap(id, add_order_item(input, context)),
        "remove_order_item" => wrap(id, remove_order_item(input, context)),
        "get_order_summary" => wrap(id, get_order_summary(context)),
        "restart_order" => {
            context.items.clear();
            context.clear_pending_selection();
            ToolOutcome::Result(ok_result(id, "Pedido reiniciado, está vacío."))
        }
        "lookup_nearby_town" => {
            let town = input.get("town").and_then(Value::as_str).unwrap_or("");
            match delivery_zone::lookup_nearby_town(town) {
                Some(found) => ToolOutcome::Result(ok_result(
                    id,
                    json!({ "found": true, "name": found.name, "delivery_cost": found.delivery_cost })
                        .to_string(),
                )),
                None => ToolOutcome::Result(ok_result(id, json!({ "found": false }).to_string())),
            }
        }
        "set_delivery_zone_armenia" => wrap(id, set_delivery_zone_armenia(input, context)),
        "set_delivery_nearby_town" => wrap(id, set_delivery_nearby_town(input, context)),
        "set_manual_delivery_cost" => {
            if actor != Actor::Advisor {
                return ToolOutcome::Result(error_result(
                    id,
                    "set_manual_delivery_cost solo se puede usar interpretando un mensaje real del asesor.",
                ));
            }
            wrap(id, set_manual_delivery_cost(input, context))
        }
        "apply_referral_code" => wrap(id, apply_referral_code(input, context)),
        "message_customer" => {
            let text = input.get("text").and_then(Value::as_str).unwrap_or("").to_string();
            if text.trim().is_empty() {
                return ToolOutcome::Result(error_result(id, "El texto no puede estar vacío."));
            }
            ToolOutcome::ResultWithAction(
                ok_result(id, "Enviado al cliente."),
                BotAction::SendText {
                    to: context.phone_number.clone(),
                    body: text,
                },
            )
        }
        "message_advisor" => {
            let text = input.get("text").and_then(Value::as_str).unwrap_or("").to_string();
            if text.trim().is_empty() {
                return ToolOutcome::Result(error_result(id, "El texto no puede estar vacío."));
            }
            ToolOutcome::ResultWithAction(
                ok_result(id, "Enviado al asesor."),
                BotAction::SendText {
                    to: context.advisor_phone.clone(),
                    body: text,
                },
            )
        }
        "finalize_checkout" => finalize_checkout(id, context),
        "confirm_advisor_availability" => {
            if actor != Actor::Advisor {
                return ToolOutcome::Result(error_result(
                    id,
                    "confirm_advisor_availability solo se puede usar interpretando un mensaje real del asesor.",
                ));
            }
            if *current_state != ConversationState::AskDeliveryCost {
                return ToolOutcome::Result(error_result(
                    id,
                    "Este caso no está esperando confirmación de disponibilidad ahora mismo.",
                ));
            }
            confirm_advisor_availability(id, input, context)
        }
        "set_payment_method" => {
            if actor != Actor::Customer {
                return ToolOutcome::Result(error_result(
                    id,
                    "set_payment_method solo se puede usar interpretando un mensaje real del cliente.",
                ));
            }
            // El asesor debe haber confirmado disponibilidad primero (estado
            // persistido antes de este turno = select_payment_method o
            // wait_receipt si esta cambiando de metodo). Si el cliente
            // intenta elegir pago mientras el caso sigue en
            // ask_delivery_cost, todavia no hay total_final confiable.
            if !matches!(
                current_state,
                ConversationState::SelectPaymentMethod | ConversationState::WaitReceipt
            ) {
                return ToolOutcome::Result(error_result(
                    id,
                    "Todavía no se puede elegir método de pago: el asesor aún no ha confirmado \
                     disponibilidad para este pedido. Avísale al cliente que estás esperando esa \
                     confirmación.",
                ));
            }
            set_payment_method(id, input, context)
        }
        "cancel_order" => cancel_order(id, context),
        _ => ToolOutcome::Result(error_result(id, format!("Herramienta desconocida: {name}"))),
    }
}

fn wrap(id: &str, (text, is_error): (String, bool)) -> ToolOutcome {
    ToolOutcome::Result(if is_error {
        error_result(id, text)
    } else {
        ok_result(id, text)
    })
}

fn set_customer_field(input: &Value, context: &mut ConversationContext) -> (String, bool) {
    let field = input.get("field").and_then(Value::as_str).unwrap_or("");
    let value = input.get("value").and_then(Value::as_str).unwrap_or("");

    let customer_field = match field {
        "name" => tools::CustomerField::Name,
        "phone" => tools::CustomerField::Phone,
        "address" => tools::CustomerField::Address,
        _ => return (format!("Campo desconocido: {field}"), true),
    };

    match tools::validate_customer_field(customer_field, value) {
        Ok(normalized) => {
            match customer_field {
                tools::CustomerField::Name => context.customer_name = Some(normalized.clone()),
                tools::CustomerField::Phone => context.customer_phone = Some(normalized.clone()),
                tools::CustomerField::Address => {
                    context.delivery_address = Some(normalized.clone())
                }
            }
            (format!("Guardado: {normalized}"), false)
        }
        Err(message) => (message, true),
    }
}

fn set_delivery_immediate(context: &mut ConversationContext) -> (String, bool) {
    let status = tools::check_business_hours();
    if !status.is_open {
        return (
            format!(
                "No se puede: fuera de horario de entrega inmediata ({}). Ofrece programar fecha y hora.",
                status.hours_text
            ),
            true,
        );
    }

    context.delivery_type = Some("immediate".to_string());
    context.scheduled_date = None;
    context.scheduled_time = None;
    ("Entrega inmediata confirmada.".to_string(), false)
}

fn set_delivery_schedule(input: &Value, context: &mut ConversationContext) -> (String, bool) {
    let date = normalize_schedule_text(input.get("date").and_then(Value::as_str).unwrap_or(""));
    let time = normalize_schedule_text(input.get("time").and_then(Value::as_str).unwrap_or(""));

    let (Some(date), Some(time)) = (date, time) else {
        return (
            "Fecha y hora deben tener texto válido (2-40 caracteres cada una).".to_string(),
            true,
        );
    };

    context.delivery_type = Some("scheduled".to_string());
    context.scheduled_date = Some(date.clone());
    context.scheduled_time = Some(time.clone());
    (
        format!("Entrega programada para {date} a las {time}."),
        false,
    )
}

fn normalize_schedule_text(raw: &str) -> Option<String> {
    let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let length = normalized.chars().count();
    if (1..=40).contains(&length) {
        Some(normalized)
    } else {
        None
    }
}

fn add_order_item(input: &Value, context: &mut ConversationContext) -> (String, bool) {
    let has_liquor = input
        .get("has_liquor")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let flavor_id = input.get("flavor_id").and_then(Value::as_str).unwrap_or("");
    let quantity_text = input
        .get("quantity")
        .map(|value| {
            value
                .as_u64()
                .map(|n| n.to_string())
                .unwrap_or_else(|| value.as_str().unwrap_or_default().to_string())
        })
        .unwrap_or_default();

    let Some(flavor) = tools::resolve_flavor(has_liquor, flavor_id) else {
        return (
            format!("flavor_id inválido: {flavor_id}. Usa get_menu para ver los ids válidos."),
            true,
        );
    };

    let quantity = match tools::validate_order_quantity(&quantity_text) {
        Ok(quantity) => quantity,
        Err(message) => return (message, true),
    };

    context.items.push(OrderItemData {
        flavor: flavor.clone(),
        has_liquor,
        quantity,
    });

    let (summary, _) = get_order_summary(context);
    (
        format!("Agregado: {quantity} x {flavor}.\n{summary}"),
        false,
    )
}

fn remove_order_item(input: &Value, context: &mut ConversationContext) -> (String, bool) {
    let flavor = input.get("flavor").and_then(Value::as_str).unwrap_or("");
    let has_liquor = input
        .get("has_liquor")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let position = context
        .items
        .iter()
        .position(|item| item.flavor == flavor && item.has_liquor == has_liquor);

    match position {
        Some(index) => {
            context.items.remove(index);
            ("Producto quitado del pedido.".to_string(), false)
        }
        None => (format!("No encontré '{flavor}' en el pedido actual."), true),
    }
}

fn get_order_summary(context: &ConversationContext) -> (String, bool) {
    if context.items.is_empty() {
        return ("El pedido está vacío.".to_string(), false);
    }

    let pedido = tools::calculate_order(&context.items);
    (checkout::render_summary(context, &pedido), false)
}

fn set_delivery_zone_armenia(input: &Value, context: &mut ConversationContext) -> (String, bool) {
    let sector = input.get("sector").and_then(Value::as_str).unwrap_or("");
    match ArmeniaZone::from_text(sector) {
        Some(zone) => {
            let cost = zone.delivery_cost();
            context.delivery_cost = Some(cost as i32);
            (
                format!(
                    "Zona {} de Armenia: domicilio ${}.",
                    zone.label(),
                    format_thousands(cost)
                ),
                false,
            )
        }
        None => (
            format!("Sector desconocido: '{sector}'. Debe ser norte, centro o sur."),
            true,
        ),
    }
}

fn set_delivery_nearby_town(input: &Value, context: &mut ConversationContext) -> (String, bool) {
    let town = input.get("town").and_then(Value::as_str).unwrap_or("");
    match delivery_zone::lookup_nearby_town(town) {
        Some(found) => {
            let total_units: u32 = context.items.iter().map(|item| item.quantity).sum();
            if total_units < MIN_UNITS_OUTSIDE_ARMENIA {
                return (
                    format!(
                        "Para {} el pedido mínimo es de {} unidades (el pedido actual tiene {}). \
                         Avísale al cliente que ese destino no aplica con esta cantidad.",
                        found.name, MIN_UNITS_OUTSIDE_ARMENIA, total_units
                    ),
                    true,
                );
            }
            context.delivery_cost = Some(found.delivery_cost as i32);
            (
                format!(
                    "{}: domicilio ${} (pedido de {} unidades, cumple el mínimo).",
                    found.name,
                    format_thousands(found.delivery_cost),
                    total_units
                ),
                false,
            )
        }
        None => (
            format!(
                "'{town}' no está en la lista de pueblos cercanos conocidos. Pídele al asesor \
                 con message_advisor que confirme el valor del domicilio (recuerda: fuera de \
                 Armenia el mínimo son {MIN_UNITS_OUTSIDE_ARMENIA} unidades) y luego usa \
                 set_manual_delivery_cost con lo que te responda."
            ),
            true,
        ),
    }
}

fn set_manual_delivery_cost(input: &Value, context: &mut ConversationContext) -> (String, bool) {
    match input.get("amount").and_then(Value::as_i64) {
        Some(amount) if amount > 0 => {
            context.delivery_cost = Some(amount as i32);
            (
                format!("Domicilio manual guardado: ${}.", format_thousands(amount as u32)),
                false,
            )
        }
        _ => ("El monto debe ser un número entero positivo.".to_string(), true),
    }
}

fn apply_referral_code(input: &Value, context: &mut ConversationContext) -> (String, bool) {
    let code = input.get("code").and_then(Value::as_str).unwrap_or("");
    let validation = tools::validate_referral_code(code);
    if !validation.is_valid {
        return (format!("Código inválido: {code}"), true);
    }

    let pedido = tools::calculate_order(&context.items);
    let Some(applied) = tools::apply_referral_code(&pedido, code) else {
        return (
            "Este pedido no tiene productos al por mayor (20+ unidades de un mismo tipo), el \
             código no aplica."
                .to_string(),
            true,
        );
    };

    context.referral_code = Some(applied.code.clone());
    context.referral_has_boost = validation.has_boost;
    context.referral_discount_total =
        Some(i32::try_from(applied.total_client_discount).unwrap_or(i32::MAX));
    context.ambassador_commission_total =
        Some(i32::try_from(applied.total_ambassador_commission).unwrap_or(i32::MAX));

    let delivery_cost = context.delivery_cost.unwrap_or(0);
    let subtotal_after_discount = i32::try_from(applied.subtotal_after_discount).unwrap_or(i32::MAX);
    let total_final = subtotal_after_discount.saturating_add(delivery_cost);
    context.total_final = Some(total_final);

    (
        format!(
            "Código aplicado. Descuento: ${}. Nuevo total con domicilio: ${}.",
            format_thousands(applied.total_client_discount),
            format_thousands(u32::try_from(total_final).unwrap_or(0))
        ),
        false,
    )
}

fn finalize_checkout(id: &str, context: &mut ConversationContext) -> ToolOutcome {
    if let Some(error) = checkout_precondition_error(context) {
        return ToolOutcome::Result(error_result(id, error));
    }

    context.payment_method = None;
    context.receipt_media_id = None;
    context.receipt_timer_started_at = None;
    context.receipt_timer_expired = false;
    context.advisor_timer_started_at = Some(chrono::Utc::now());
    context.advisor_timer_expired = false;

    let timeout = if context.delivery_type.as_deref() == Some("scheduled") {
        ADVISOR_RESPONSE_TIMEOUT
    } else {
        ADVISOR_RESPONSE_TIMEOUT
    };

    let actions = vec![
        BotAction::FinalizeCurrentOrder {
            status: "pending_advisor".to_string(),
        },
        BotAction::BindAdvisorSession {
            advisor_phone: context.advisor_phone.clone(),
            target_phone: context.phone_number.clone(),
        },
        BotAction::StartTimer {
            timer_type: TimerType::AdvisorResponse,
            phone: context.phone_number.clone(),
            duration: timeout,
        },
    ];

    ToolOutcome::ResultWithStateChange(
        ok_result(id, "Pedido enviado a revisión del asesor."),
        ConversationState::AskDeliveryCost,
        actions,
    )
}

fn checkout_precondition_error(context: &ConversationContext) -> Option<String> {
    let mut missing = Vec::new();

    if context.items.is_empty() {
        missing.push("al menos un producto en el pedido".to_string());
    }
    if context.customer_name.is_none() {
        missing.push("nombre del cliente".to_string());
    }
    if context.customer_phone.is_none() {
        missing.push("teléfono del cliente".to_string());
    }
    if context.delivery_address.is_none() {
        missing.push("dirección de entrega".to_string());
    }

    match context.delivery_type.as_deref() {
        Some("immediate") => {
            if !tools::check_business_hours().is_open {
                missing.push(
                    "entrega inmediata fuera de horario: ofrece programar fecha y hora en su lugar"
                        .to_string(),
                );
            }
        }
        Some("scheduled") => {
            if context.scheduled_date.is_none() {
                missing.push("fecha programada".to_string());
            }
            if context.scheduled_time.is_none() {
                missing.push("hora programada".to_string());
            }
        }
        _ => missing.push("tipo de entrega (inmediata o programada)".to_string()),
    }

    if missing.is_empty() {
        None
    } else {
        Some(format!(
            "Todavía falta antes de poder finalizar: {}.",
            missing.join(", ")
        ))
    }
}

fn confirm_advisor_availability(
    id: &str,
    input: &Value,
    context: &mut ConversationContext,
) -> ToolOutcome {
    let available = input.get("available").and_then(Value::as_bool).unwrap_or(false);

    context.advisor_timer_started_at = None;
    context.advisor_timer_expired = false;

    if !available {
        let mut actions = vec![
            BotAction::CancelTimer {
                timer_type: TimerType::AdvisorResponse,
                phone: context.phone_number.clone(),
            },
            BotAction::ClearAdvisorSession {
                advisor_phone: context.advisor_phone.clone(),
            },
        ];
        if context.current_order_id.is_some() {
            actions.push(BotAction::UpsertDraftOrder {
                status: "manual_followup".to_string(),
            });
        }
        actions.push(BotAction::ResetConversation {
            phone: context.phone_number.clone(),
        });

        return ToolOutcome::ResultWithStateChange(
            ok_result(id, "Registrado: el asesor no puede atender este pedido."),
            ConversationState::MainMenu,
            actions,
        );
    }

    let Some(delivery_cost) = context.delivery_cost else {
        return ToolOutcome::Result(error_result(
            id,
            "Todavía no hay costo de domicilio definido. Usa set_delivery_zone_armenia, \
             set_delivery_nearby_town o set_manual_delivery_cost primero.",
        ));
    };

    let pedido = tools::calculate_order(&context.items);
    let total_final = i32::try_from(pedido.total_estimado).unwrap_or(i32::MAX) + delivery_cost;
    context.total_final = Some(total_final);

    let total_units_purchased: i32 = context.items.iter().map(|item| item.quantity as i32).sum();

    let mut actions = vec![
        BotAction::UpdateCurrentOrderDeliveryCost {
            delivery_cost,
            total_final,
            status: "draft_payment".to_string(),
        },
        BotAction::CancelTimer {
            timer_type: TimerType::AdvisorResponse,
            phone: context.phone_number.clone(),
        },
        BotAction::ClearAdvisorSession {
            advisor_phone: context.advisor_phone.clone(),
        },
    ];

    // Actualizar totales de cliente y analytics de referral si aplica
    if context.current_order_id.is_some() {
        actions.push(BotAction::UpdateCustomerAndAnalytics {
            phone_number_meta: context.phone_number.clone(),
            total_spent_cop: total_final,
            total_units_purchased,
            referral_code: context.referral_code.clone(),
            referral_discount_cop: context.referral_discount_total,
            ambassador_commission_cop: context.ambassador_commission_total,
        });
    }

    ToolOutcome::ResultWithStateChange(
        ok_result(id, "Disponibilidad confirmada, listo para pago."),
        ConversationState::SelectPaymentMethod,
        actions,
    )
}

fn set_payment_method(id: &str, input: &Value, context: &mut ConversationContext) -> ToolOutcome {
    let method = input.get("method").and_then(Value::as_str).unwrap_or("");

    match method {
        "cash_on_delivery" => {
            context.payment_method = Some("cash_on_delivery".to_string());
            context.receipt_media_id = None;
            let summary = advisor_case_summary(context);
            let actions = vec![
                BotAction::UpsertDraftOrder {
                    status: "confirmed".to_string(),
                },
                BotAction::SendText {
                    to: context.advisor_phone.clone(),
                    body: format!("Pedido confirmado (contra entrega):\n\n{summary}"),
                },
                BotAction::ResetConversation {
                    phone: context.phone_number.clone(),
                },
            ];
            ToolOutcome::ResultWithStateChange(
                ok_result(id, "Pago contra entrega registrado."),
                ConversationState::MainMenu,
                actions,
            )
        }
        "transfer" => {
            context.payment_method = Some("transfer".to_string());
            context.receipt_timer_started_at = Some(chrono::Utc::now());
            context.receipt_timer_expired = false;
            let actions = vec![
                BotAction::UpsertDraftOrder {
                    status: "waiting_receipt".to_string(),
                },
                BotAction::SendTransferInstructions {
                    to: context.phone_number.clone(),
                },
                BotAction::StartTimer {
                    timer_type: TimerType::ReceiptUpload,
                    phone: context.phone_number.clone(),
                    duration: RECEIPT_TIMEOUT,
                },
            ];
            ToolOutcome::ResultWithStateChange(
                ok_result(id, "Transferencia seleccionada, esperando comprobante."),
                ConversationState::WaitReceipt,
                actions,
            )
        }
        _ => ToolOutcome::Result(error_result(
            id,
            format!("Método de pago desconocido: {method}"),
        )),
    }
}

fn cancel_order(id: &str, context: &mut ConversationContext) -> ToolOutcome {
    let order_id = context.current_order_id;
    context.items.clear();
    context.payment_method = None;
    context.clear_referral_data();
    context.delivery_cost = None;
    context.total_final = None;
    context.receipt_media_id = None;
    context.receipt_timer_started_at = None;
    context.current_order_id = None;
    context.receipt_timer_expired = false;
    context.clear_pending_selection();

    let mut actions = vec![BotAction::CancelTimer {
        timer_type: TimerType::ReceiptUpload,
        phone: context.phone_number.clone(),
    }];
    if let Some(order_id) = order_id {
        actions.push(BotAction::CancelCurrentOrder { order_id });
    }
    actions.push(BotAction::ResetConversation {
        phone: context.phone_number.clone(),
    });

    ToolOutcome::ResultWithStateChange(
        ok_result(id, "Pedido cancelado."),
        ConversationState::MainMenu,
        actions,
    )
}

fn advisor_case_summary(context: &ConversationContext) -> String {
    let pedido = tools::calculate_order(&context.items);
    let items_text = checkout::render_items(&pedido.items_detalle);
    let delivery = match context.delivery_type.as_deref() {
        Some("immediate") => "Inmediata".to_string(),
        Some("scheduled") => format!(
            "Programada ({} {})",
            context.scheduled_date.as_deref().unwrap_or("?"),
            context.scheduled_time.as_deref().unwrap_or("?")
        ),
        _ => "Pendiente".to_string(),
    };
    let delivery_cost = context.delivery_cost.unwrap_or(0);
    let total_final = context
        .total_final
        .unwrap_or_else(|| i32::try_from(pedido.total_estimado).unwrap_or(0) + delivery_cost);

    format!(
        "Cliente: {}\nTeléfono: {}\nDirección: {}\nEntrega: {}\n\nItems:\n{}\n\nDomicilio: ${}\nTotal final: ${}",
        context.customer_name.as_deref().unwrap_or("pendiente"),
        context.customer_phone.as_deref().unwrap_or("pendiente"),
        context.delivery_address.as_deref().unwrap_or("pendiente"),
        delivery,
        items_text,
        format_thousands(u32::try_from(delivery_cost).unwrap_or(0)),
        format_thousands(u32::try_from(total_final).unwrap_or(0)),
    )
}

fn format_thousands(value: u32) -> String {
    let digits = value.to_string();
    let mut rendered = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            rendered.push('.');
        }
        rendered.push(ch);
    }
    rendered.chars().rev().collect()
}

fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "get_menu".to_string(),
            description: "Devuelve el texto del menú vigente y los flavor_id válidos para con/sin licor.".to_string(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDefinition {
            name: "check_business_hours".to_string(),
            description: "Indica si ahora mismo está dentro del horario de entrega inmediata (8:00 AM - 11:00 PM Bogotá) y el texto del horario.".to_string(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDefinition {
            name: "show_menu_image".to_string(),
            description: "Envía la imagen del menú al cliente (además de cualquier mensaje que le mandes).".to_string(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDefinition {
            name: "set_customer_field".to_string(),
            description: "Guarda nombre, teléfono o dirección del cliente ya validados.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "field": { "type": "string", "enum": ["name", "phone", "address"] },
                    "value": { "type": "string" }
                },
                "required": ["field", "value"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "set_delivery_immediate".to_string(),
            description: "Marca el pedido como entrega inmediata. Falla si está fuera de horario.".to_string(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDefinition {
            name: "set_delivery_schedule".to_string(),
            description: "Marca el pedido como programado, guardando fecha y hora tal como las dijo el cliente (texto libre).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string" },
                    "time": { "type": "string" }
                },
                "required": ["date", "time"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "add_order_item".to_string(),
            description: "Agrega un producto al pedido usando un flavor_id válido de get_menu.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "has_liquor": { "type": "boolean" },
                    "flavor_id": { "type": "string" },
                    "quantity": { "type": "integer", "minimum": 1, "maximum": 999 }
                },
                "required": ["has_liquor", "flavor_id", "quantity"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "remove_order_item".to_string(),
            description: "Quita del pedido el primer item que coincida con flavor y has_liquor.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "flavor": { "type": "string" },
                    "has_liquor": { "type": "boolean" }
                },
                "required": ["flavor", "has_liquor"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "get_order_summary".to_string(),
            description: "Devuelve el resumen actual del pedido con precios reales (items, subtotal).".to_string(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDefinition {
            name: "restart_order".to_string(),
            description: "Borra todos los productos del pedido actual. Confirma con el cliente antes de llamarla.".to_string(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDefinition {
            name: "lookup_nearby_town".to_string(),
            description: "Busca si un municipio/pueblo esta en la lista conocida de destinos cercanos a Armenia y su costo de domicilio fijo.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "town": { "type": "string" } },
                "required": ["town"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "set_delivery_zone_armenia".to_string(),
            description: "Fija el domicilio automatico segun la zona de Armenia (norte/centro/sur). Usar solo cuando la direccion es dentro de Armenia.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "sector": { "type": "string", "enum": ["norte", "centro", "sur"] } },
                "required": ["sector"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "set_delivery_nearby_town".to_string(),
            description: "Fija el domicilio automatico para un pueblo cercano conocido (requiere que el pedido ya tenga 20+ unidades).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "town": { "type": "string" } },
                "required": ["town"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "set_manual_delivery_cost".to_string(),
            description: "Guarda un costo de domicilio que te dio el asesor manualmente (destinos fuera de la lista conocida).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "amount": { "type": "integer", "minimum": 1 } },
                "required": ["amount"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "apply_referral_code".to_string(),
            description: "Valida y aplica un codigo de referido/embajador al pedido (solo aplica si hay 20+ unidades de un mismo tipo).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "code": { "type": "string" } },
                "required": ["code"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "message_customer".to_string(),
            description: "Envía un mensaje de texto al CLIENTE. Úsala siempre que quieras decirle algo al cliente.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "message_advisor".to_string(),
            description: "Envía un mensaje de texto al ASESOR humano. Úsala siempre que quieras decirle algo al asesor.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "finalize_checkout".to_string(),
            description: "Envía el pedido a revisión del asesor. Solo llamar después de que el cliente confirme explícitamente y con todos los datos completos.".to_string(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDefinition {
            name: "confirm_advisor_availability".to_string(),
            description: "Registra si el asesor confirmó que puede (o no) atender el pedido. Solo usar interpretando una respuesta real del asesor.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "available": { "type": "boolean" } },
                "required": ["available"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "set_payment_method".to_string(),
            description: "Registra el método de pago elegido por el cliente (cash_on_delivery o transfer).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "method": { "type": "string", "enum": ["cash_on_delivery", "transfer"] } },
                "required": ["method"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "cancel_order".to_string(),
            description: "Cancela el pedido actual por completo. Confirma con el cliente antes de llamarla.".to_string(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
    ]
}
