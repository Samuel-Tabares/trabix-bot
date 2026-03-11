use std::time::Duration;

use crate::{
    bot::{
        pricing::{calcular_pedido, ItemCalculated},
        state_machine::{
            BotAction, ConversationContext, ConversationState, TimerType, TransitionResult,
            UserInput,
        },
        timers::RECEIPT_TIMEOUT,
    },
    whatsapp::types::{Button, ButtonReplyPayload, ListRow, ListSection},
};

use super::{data_collect, menu, order};

const CASH_ON_DELIVERY: &str = "cash_on_delivery";
const PAY_NOW: &str = "pay_now";
const MODIFY_ORDER: &str = "modify_order";
const CANCEL_ORDER: &str = "cancel_order";
const CHANGE_PAYMENT_METHOD: &str = "change_payment_method";
const CONFIRM_ADDRESS: &str = "confirm_address";
const CHANGE_ADDRESS: &str = "change_address";
const BACK_MAIN_MENU: &str = "back_main_menu";

pub fn handle_show_summary(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(CASH_ON_DELIVERY) => {
            context.payment_method = Some(CASH_ON_DELIVERY.to_string());
            context.receipt_media_id = None;
            context.receipt_timer_started_at = None;
            context.receipt_timer_expired = false;
            context.editing_address = false;

            let mut actions = vec![BotAction::UpsertDraftOrder {
                status: "draft_payment".to_string(),
            }];
            actions.extend(confirm_address_actions(context));

            Ok((ConversationState::ConfirmAddress, actions))
        }
        Some(PAY_NOW) => {
            context.payment_method = Some("transfer".to_string());
            context.receipt_media_id = None;
            context.receipt_timer_started_at = Some(chrono::Utc::now());
            context.receipt_timer_expired = false;
            context.editing_address = false;

            Ok((
                ConversationState::WaitReceipt,
                wait_receipt_entry_actions(context),
            ))
        }
        Some(MODIFY_ORDER) => {
            let cancel_action = context
                .current_order_id
                .map(|order_id| BotAction::CancelCurrentOrder { order_id });
            context.items.clear();
            context.payment_method = None;
            context.receipt_media_id = None;
            context.receipt_timer_started_at = None;
            context.current_order_id = None;
            context.editing_address = false;
            context.receipt_timer_expired = false;
            context.clear_pending_selection();

            let mut actions = Vec::new();
            if let Some(action) = cancel_action {
                actions.push(action);
            }
            actions.push(BotAction::SendText {
                to: context.phone_number.clone(),
                body: "Perfecto. Vamos a reconstruir tu pedido desde los sabores.".to_string(),
            });
            actions.extend(order::select_type_actions(&context.phone_number));

            Ok((ConversationState::SelectType, actions))
        }
        Some(CANCEL_ORDER) => Ok(cancel_order_transition(context)),
        _ => Ok((
            ConversationState::ShowSummary,
            show_summary_actions(context),
        )),
    }
}

pub fn handle_wait_receipt(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    if context.receipt_timer_expired {
        return match selection_id(input).as_deref() {
            Some(CHANGE_PAYMENT_METHOD) => {
                context.payment_method = None;
                context.receipt_media_id = None;
                context.receipt_timer_started_at = None;
                context.receipt_timer_expired = false;

                Ok((
                    ConversationState::ShowSummary,
                    show_summary_actions(context),
                ))
            }
            Some(CANCEL_ORDER) => Ok(cancel_order_transition(context)),
            _ => Ok((
                ConversationState::WaitReceipt,
                receipt_timeout_repeat_actions(&context.phone_number),
            )),
        };
    }

    match input {
        UserInput::ImageMessage(media_id) => {
            context.receipt_media_id = Some(media_id.clone());
            context.receipt_timer_started_at = None;
            context.receipt_timer_expired = false;
            context.editing_address = false;

            let mut actions = vec![
                BotAction::CancelTimer {
                    timer_type: TimerType::ReceiptUpload,
                    phone: context.phone_number.clone(),
                },
                BotAction::UpsertDraftOrder {
                    status: "draft_payment".to_string(),
                },
            ];
            actions.extend(confirm_address_actions(context));

            Ok((ConversationState::ConfirmAddress, actions))
        }
        _ => Ok((
            ConversationState::WaitReceipt,
            vec![BotAction::SendText {
                to: context.phone_number.clone(),
                body: "Para validar el pago necesito una imagen del comprobante. Envíala como foto por este chat.".to_string(),
            }],
        )),
    }
}

pub fn handle_confirm_address(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    if context.editing_address {
        return match input {
            UserInput::TextMessage(text) => match data_collect::validate_address(text) {
                Ok(address) => {
                    context.delivery_address = Some(address);
                    context.editing_address = false;

                    Ok((
                        ConversationState::ConfirmAddress,
                        confirm_address_actions(context),
                    ))
                }
                Err(message) => Ok((
                    ConversationState::ConfirmAddress,
                    vec![
                        BotAction::SendText {
                            to: context.phone_number.clone(),
                            body: message,
                        },
                        BotAction::SendText {
                            to: context.phone_number.clone(),
                            body: "Escribe la nueva dirección completa para actualizar el pedido."
                                .to_string(),
                        },
                    ],
                )),
            },
            _ => Ok((
                ConversationState::ConfirmAddress,
                vec![BotAction::SendText {
                    to: context.phone_number.clone(),
                    body: "Escribe la nueva dirección en texto para continuar.".to_string(),
                }],
            )),
        };
    }

    match selection_id(input).as_deref() {
        Some(CONFIRM_ADDRESS) => Ok((
            ConversationState::WaitAdvisorResponse,
            vec![
                BotAction::FinalizeCurrentOrder {
                    status: "pending_advisor".to_string(),
                },
                BotAction::SendText {
                    to: context.phone_number.clone(),
                    body: "Tu pedido quedó registrado y será confirmado por un asesor.".to_string(),
                },
            ],
        )),
        Some(CHANGE_ADDRESS) => {
            context.editing_address = true;
            Ok((
                ConversationState::ConfirmAddress,
                vec![BotAction::SendText {
                    to: context.phone_number.clone(),
                    body: "Escribe la nueva dirección completa para actualizar el pedido."
                        .to_string(),
                }],
            ))
        }
        _ => Ok((
            ConversationState::ConfirmAddress,
            confirm_address_actions(context),
        )),
    }
}

pub fn handle_wait_advisor_response(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    if matches!(input, UserInput::TextMessage(text) if text.trim().eq_ignore_ascii_case("menu")) {
        return Ok((ConversationState::MainMenu, {
            let mut actions = vec![BotAction::ResetConversation {
                phone: context.phone_number.clone(),
            }];
            actions.extend(menu::main_menu_actions(&context.phone_number));
            actions
        }));
    }

    if matches!(selection_id(input).as_deref(), Some(BACK_MAIN_MENU)) {
        return Ok((ConversationState::MainMenu, {
            let mut actions = vec![BotAction::ResetConversation {
                phone: context.phone_number.clone(),
            }];
            actions.extend(menu::main_menu_actions(&context.phone_number));
            actions
        }));
    }

    if matches!(selection_id(input).as_deref(), Some(CANCEL_ORDER)) {
        return Ok(cancel_order_transition(context));
    }

    Ok((
        ConversationState::WaitAdvisorResponse,
        vec![
            BotAction::SendText {
                to: context.phone_number.clone(),
                body: "Tu pedido ya está registrado. Un asesor te escribirá para confirmar el domicilio y el cierre final.".to_string(),
            },
            BotAction::SendButtons {
                to: context.phone_number.clone(),
                body: "Si necesitas salir del flujo mientras llega el asesor, puedes volver al menú.".to_string(),
                buttons: vec![
                    reply_button(CANCEL_ORDER, "Cancelar"),
                    reply_button(BACK_MAIN_MENU, "Volver al menu"),
                ],
            },
        ],
    ))
}

pub fn handle_order_complete(context: &mut ConversationContext) -> TransitionResult {
    let mut actions = vec![BotAction::ResetConversation {
        phone: context.phone_number.clone(),
    }];
    actions.extend(menu::main_menu_actions(&context.phone_number));

    Ok((ConversationState::MainMenu, actions))
}

pub fn show_summary_actions(context: &ConversationContext) -> Vec<BotAction> {
    let pedido = calcular_pedido(&context.items);
    vec![
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: render_summary(context, &pedido),
        },
        BotAction::SendList {
            to: context.phone_number.clone(),
            body: "Selecciona cómo quieres continuar con tu pedido.".to_string(),
            button_text: "Ver opciones".to_string(),
            sections: vec![ListSection {
                title: "Pago y pedido".to_string(),
                rows: vec![
                    ListRow {
                        id: CASH_ON_DELIVERY.to_string(),
                        title: "Contra Entrega".to_string(),
                        description: "Pagar al recibir el pedido".to_string(),
                    },
                    ListRow {
                        id: PAY_NOW.to_string(),
                        title: "Pago Ahora".to_string(),
                        description: "Transferencia y envío de comprobante".to_string(),
                    },
                    ListRow {
                        id: MODIFY_ORDER.to_string(),
                        title: "Modificar Pedido".to_string(),
                        description: "Borrar items y volver a elegir sabores".to_string(),
                    },
                    ListRow {
                        id: CANCEL_ORDER.to_string(),
                        title: "Cancelar Pedido".to_string(),
                        description: "Salir y volver al menú principal".to_string(),
                    },
                ],
            }],
        },
    ]
}

pub fn confirm_address_actions(context: &ConversationContext) -> Vec<BotAction> {
    vec![BotAction::SendButtons {
        to: context.phone_number.clone(),
        body: format!(
            "Tu dirección de entrega es:\n{}\n\n¿Es correcta?",
            context
                .delivery_address
                .as_deref()
                .unwrap_or("pendiente por confirmar")
        ),
        buttons: vec![
            reply_button(CONFIRM_ADDRESS, "Si, correcta"),
            reply_button(CHANGE_ADDRESS, "Cambiar direccion"),
        ],
    }]
}

fn wait_receipt_entry_actions(context: &ConversationContext) -> Vec<BotAction> {
    vec![
        BotAction::UpsertDraftOrder {
            status: "waiting_receipt".to_string(),
        },
        BotAction::SendTransferInstructions {
            to: context.phone_number.clone(),
        },
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: "Envía una foto del comprobante en los próximos 10 minutos para continuar."
                .to_string(),
        },
        BotAction::StartTimer {
            timer_type: TimerType::ReceiptUpload,
            phone: context.phone_number.clone(),
            duration: Duration::from_secs(RECEIPT_TIMEOUT.as_secs()),
        },
    ]
}

fn receipt_timeout_repeat_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendButtons {
        to: phone.to_string(),
        body: "El tiempo para el comprobante ya venció. Puedes cambiar la forma de pago o cancelar el pedido.".to_string(),
        buttons: vec![
            reply_button(CHANGE_PAYMENT_METHOD, "Cambiar pago"),
            reply_button(CANCEL_ORDER, "Cancelar"),
        ],
    }]
}

fn cancel_order_transition(
    context: &mut ConversationContext,
) -> (ConversationState, Vec<BotAction>) {
    let cancel_action = context
        .current_order_id
        .map(|order_id| BotAction::CancelCurrentOrder { order_id });

    context.items.clear();
    context.payment_method = None;
    context.receipt_media_id = None;
    context.receipt_timer_started_at = None;
    context.current_order_id = None;
    context.editing_address = false;
    context.receipt_timer_expired = false;
    context.clear_pending_selection();

    let mut actions = vec![BotAction::CancelTimer {
        timer_type: TimerType::ReceiptUpload,
        phone: context.phone_number.clone(),
    }];
    if let Some(action) = cancel_action {
        actions.push(action);
    }
    actions.push(BotAction::ResetConversation {
        phone: context.phone_number.clone(),
    });
    actions.extend(menu::main_menu_actions(&context.phone_number));

    (ConversationState::MainMenu, actions)
}

fn render_summary(
    context: &ConversationContext,
    pedido: &crate::bot::pricing::PedidoCalculado,
) -> String {
    let entrega = match context.delivery_type.as_deref() {
        Some("immediate") => "Inmediata".to_string(),
        Some("scheduled") => format!(
            "Programada\nFecha: {}\nHora: {}",
            context.scheduled_date.as_deref().unwrap_or("pendiente"),
            context.scheduled_time.as_deref().unwrap_or("pendiente")
        ),
        Some(other) => other.to_string(),
        None => "Pendiente".to_string(),
    };

    format!(
        "RESUMEN DEL PEDIDO\n\nCliente: {}\nTeléfono: {}\nDirección: {}\nEntrega: {}\n\nItems:\n{}\n\nTotal estimado: {}\n\nNota: el domicilio no está incluido. Un asesor lo define después de revisar tu pedido.",
        context.customer_name.as_deref().unwrap_or("pendiente"),
        context.customer_phone.as_deref().unwrap_or("pendiente"),
        context.delivery_address.as_deref().unwrap_or("pendiente"),
        entrega,
        render_items(&pedido.items_detalle),
        format_currency(pedido.total_estimado),
    )
}

fn render_items(items: &[ItemCalculated]) -> String {
    if items.is_empty() {
        return "- Sin items".to_string();
    }

    items
        .iter()
        .map(|item| {
            let tipo = if item.has_liquor {
                "con licor"
            } else {
                "sin licor"
            };
            let modo = if item.is_wholesale { "mayor" } else { "detal" };
            let promo = if item.promo_units > 0 {
                format!(" | promo pares en {} unidad(es)", item.promo_units)
            } else {
                String::new()
            };

            format!(
                "- {} x {} ({}, {}) -> {}{}",
                item.quantity,
                item.flavor,
                tipo,
                modo,
                format_currency(item.subtotal),
                promo
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn selection_id(input: &UserInput) -> Option<String> {
    match input {
        UserInput::ButtonPress(id) | UserInput::ListSelection(id) => Some(id.clone()),
        _ => None,
    }
}

fn reply_button(id: &str, title: &str) -> Button {
    Button {
        kind: "reply".to_string(),
        reply: ButtonReplyPayload {
            id: id.to_string(),
            title: title.to_string(),
        },
    }
}

fn format_currency(value: u32) -> String {
    let digits = value.to_string();
    let mut rendered = String::with_capacity(digits.len() + (digits.len() / 3) + 1);

    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            rendered.push('.');
        }
        rendered.push(ch);
    }

    format!("${}", rendered.chars().rev().collect::<String>())
}

#[cfg(test)]
mod tests {
    use crate::bot::state_machine::{ConversationContext, ConversationState, UserInput};

    use super::{handle_confirm_address, handle_show_summary, handle_wait_receipt};

    fn context() -> ConversationContext {
        ConversationContext {
            phone_number: "573001234567".to_string(),
            customer_name: Some("Ana".to_string()),
            customer_phone: Some("3001234567".to_string()),
            delivery_address: Some("Cra 15 #20-30 Armenia".to_string()),
            items: vec![
                crate::db::models::OrderItemData {
                    flavor: "maracuya".to_string(),
                    has_liquor: true,
                    quantity: 2,
                },
                crate::db::models::OrderItemData {
                    flavor: "mora".to_string(),
                    has_liquor: false,
                    quantity: 1,
                },
            ],
            delivery_type: Some("immediate".to_string()),
            scheduled_date: None,
            scheduled_time: None,
            payment_method: None,
            receipt_media_id: None,
            receipt_timer_started_at: None,
            current_order_id: Some(7),
            editing_address: false,
            receipt_timer_expired: false,
            pending_has_liquor: None,
            pending_flavor: None,
        }
    }

    #[test]
    fn show_summary_pay_now_moves_to_wait_receipt() {
        let mut context = context();
        let (state, actions) = handle_show_summary(
            &UserInput::ListSelection("pay_now".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::WaitReceipt);
        assert_eq!(context.payment_method.as_deref(), Some("transfer"));
        assert!(context.receipt_timer_started_at.is_some());
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::StartTimer { .. }
        )));
    }

    #[test]
    fn wait_receipt_accepts_only_image_while_timer_is_active() {
        let mut context = context();
        context.payment_method = Some("transfer".to_string());

        let (state, _) = handle_wait_receipt(
            &UserInput::ImageMessage("media-123".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::ConfirmAddress);
        assert_eq!(context.receipt_media_id.as_deref(), Some("media-123"));
        assert_eq!(context.receipt_timer_started_at, None);
    }

    #[test]
    fn wait_receipt_after_timeout_offers_change_or_cancel() {
        let mut context = context();
        context.payment_method = Some("transfer".to_string());
        context.receipt_timer_expired = true;
        context.receipt_timer_started_at = Some(chrono::Utc::now());

        let (state, actions) =
            handle_wait_receipt(&UserInput::TextMessage("hola".to_string()), &mut context)
                .expect("transition");

        assert_eq!(state, ConversationState::WaitReceipt);
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::SendButtons { .. }
        )));

        let (state, _) = handle_wait_receipt(
            &UserInput::ButtonPress("change_payment_method".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::ShowSummary);
        assert_eq!(context.payment_method, None);
        assert_eq!(context.receipt_timer_started_at, None);
        assert!(!context.receipt_timer_expired);
    }

    #[test]
    fn confirm_address_can_switch_to_edit_mode_and_save_address() {
        let mut context = context();
        let (state, _) = handle_confirm_address(
            &UserInput::ButtonPress("change_address".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::ConfirmAddress);
        assert!(context.editing_address);

        let (state, _) = handle_confirm_address(
            &UserInput::TextMessage("Av 14 #10-20 Armenia".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::ConfirmAddress);
        assert_eq!(
            context.delivery_address.as_deref(),
            Some("Av 14 #10-20 Armenia")
        );
        assert!(!context.editing_address);
    }
}
