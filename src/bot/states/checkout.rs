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
    messages::{client_messages, render_template},
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
                body: client_messages().checkout.modify_order_text.clone(),
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
                body: client_messages().checkout.receipt_image_required.clone(),
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
                            body: client_messages().checkout.change_address_prompt.clone(),
                        },
                    ],
                )),
            },
            _ => Ok((
                ConversationState::ConfirmAddress,
                vec![BotAction::SendText {
                    to: context.phone_number.clone(),
                    body: client_messages().checkout.change_address_non_text.clone(),
                }],
            )),
        };
    }

    match selection_id(input).as_deref() {
        Some(CONFIRM_ADDRESS) => {
            let (state, actions) =
                super::advisor::handoff_order_after_address_confirmation(context);
            Ok((state, actions))
        }
        Some(CHANGE_ADDRESS) => {
            context.editing_address = true;
            Ok((
                ConversationState::ConfirmAddress,
                vec![BotAction::SendText {
                    to: context.phone_number.clone(),
                    body: client_messages().checkout.change_address_prompt.clone(),
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
    super::advisor::handle_client_waiting_state(
        &ConversationState::WaitAdvisorResponse,
        input,
        context,
    )
}

pub fn handle_order_complete(context: &mut ConversationContext) -> TransitionResult {
    let mut actions = vec![BotAction::ResetConversation {
        phone: context.phone_number.clone(),
    }];
    actions.extend(menu::main_menu_actions(&context.phone_number));

    Ok((ConversationState::MainMenu, actions))
}

pub fn show_summary_actions(context: &ConversationContext) -> Vec<BotAction> {
    let messages = &client_messages().checkout;
    let pedido = calcular_pedido(&context.items);
    vec![
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: render_summary(context, &pedido),
        },
        BotAction::SendList {
            to: context.phone_number.clone(),
            body: messages.summary_list_body.clone(),
            button_text: messages.summary_list_button_text.clone(),
            sections: vec![ListSection {
                title: messages.summary_section_title.clone(),
                rows: vec![
                    ListRow {
                        id: CASH_ON_DELIVERY.to_string(),
                        title: messages.cash_on_delivery_title.clone(),
                        description: messages.cash_on_delivery_description.clone(),
                    },
                    ListRow {
                        id: PAY_NOW.to_string(),
                        title: messages.pay_now_title.clone(),
                        description: messages.pay_now_description.clone(),
                    },
                    ListRow {
                        id: MODIFY_ORDER.to_string(),
                        title: messages.modify_order_title.clone(),
                        description: messages.modify_order_description.clone(),
                    },
                    ListRow {
                        id: CANCEL_ORDER.to_string(),
                        title: messages.cancel_order_title.clone(),
                        description: messages.cancel_order_description.clone(),
                    },
                ],
            }],
        },
    ]
}

pub fn confirm_address_actions(context: &ConversationContext) -> Vec<BotAction> {
    let messages = &client_messages().checkout;
    vec![BotAction::SendButtons {
        to: context.phone_number.clone(),
        body: render_template(
            &messages.confirm_address_template,
            &[(
                "address",
                context
                    .delivery_address
                    .as_deref()
                    .unwrap_or("pendiente por confirmar"),
            )],
        ),
        buttons: vec![
            reply_button(CONFIRM_ADDRESS, &messages.confirm_address_button),
            reply_button(CHANGE_ADDRESS, &messages.change_address_button),
        ],
    }]
}

pub fn change_address_prompt_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages().checkout.change_address_prompt.clone(),
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
            body: client_messages().checkout.wait_receipt_prompt.clone(),
        },
        BotAction::StartTimer {
            timer_type: TimerType::ReceiptUpload,
            phone: context.phone_number.clone(),
            duration: Duration::from_secs(RECEIPT_TIMEOUT.as_secs()),
        },
    ]
}

fn receipt_timeout_repeat_actions(phone: &str) -> Vec<BotAction> {
    let messages = &client_messages().checkout;
    vec![BotAction::SendButtons {
        to: phone.to_string(),
        body: messages.receipt_timeout_body.clone(),
        buttons: vec![
            reply_button(
                CHANGE_PAYMENT_METHOD,
                &messages.receipt_timeout_change_payment_button,
            ),
            reply_button(CANCEL_ORDER, &messages.receipt_timeout_cancel_button),
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
    let messages = &client_messages().checkout;
    let entrega = match context.delivery_type.as_deref() {
        Some("immediate") => messages.summary_delivery_immediate.clone(),
        Some("scheduled") => render_template(
            &messages.summary_delivery_scheduled_template,
            &[
                (
                    "date",
                    context.scheduled_date.as_deref().unwrap_or("pendiente"),
                ),
                (
                    "time",
                    context.scheduled_time.as_deref().unwrap_or("pendiente"),
                ),
            ],
        ),
        Some(other) => other.to_string(),
        None => messages.summary_delivery_pending.clone(),
    };

    render_template(
        &messages.summary_template,
        &[
            (
                "customer_name",
                context.customer_name.as_deref().unwrap_or("pendiente"),
            ),
            (
                "customer_phone",
                context.customer_phone.as_deref().unwrap_or("pendiente"),
            ),
            (
                "delivery_address",
                context.delivery_address.as_deref().unwrap_or("pendiente"),
            ),
            ("delivery", &entrega),
            ("items", &render_items(&pedido.items_detalle)),
            ("total_estimated", &format_currency(pedido.total_estimado)),
        ],
    )
}

fn render_items(items: &[ItemCalculated]) -> String {
    let messages = &client_messages().checkout;
    if items.is_empty() {
        return messages.summary_items_empty.clone();
    }

    items
        .iter()
        .map(|item| {
            let tipo = if item.has_liquor {
                messages.summary_item_kind_with_liquor.as_str()
            } else {
                messages.summary_item_kind_without_liquor.as_str()
            };
            let modo = if item.is_wholesale {
                messages.summary_item_mode_wholesale.as_str()
            } else {
                messages.summary_item_mode_detail.as_str()
            };
            let promo = if item.promo_units > 0 {
                render_template(
                    &messages.summary_item_promo_template,
                    &[("promo_units", &item.promo_units.to_string())],
                )
            } else {
                String::new()
            };

            render_template(
                &messages.summary_item_line_template,
                &[
                    ("quantity", &item.quantity.to_string()),
                    ("flavor", &item.flavor),
                    ("kind", tipo),
                    ("mode", modo),
                    ("subtotal", &format_currency(item.subtotal)),
                    ("promo", &promo),
                ],
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
            advisor_phone: "573009999999".to_string(),
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
            advisor_target_phone: None,
            advisor_timer_started_at: None,
            advisor_timer_expired: false,
            relay_timer_started_at: None,
            relay_kind: None,
            advisor_proposed_hour: None,
            client_counter_hour: None,
            schedule_resume_target: None,
            current_order_id: Some(7),
            editing_address: false,
            receipt_timer_expired: false,
            pending_has_liquor: None,
            pending_flavor: None,
            conversation_abandon_started_at: None,
            conversation_abandon_reminder_sent: false,
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
