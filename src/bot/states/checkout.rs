use std::time::Duration;

use crate::{
    bot::{
        pricing::{
            calcular_pedido, calcular_referido, has_wholesale_bucket, ItemCalculated,
            PedidoCalculado, ReferralApplied,
        },
        state_machine::{
            BotAction, ConversationContext, ConversationState, TimerType, TransitionResult,
            UserInput,
        },
        timers::RECEIPT_TIMEOUT,
    },
    messages::{client_messages, render_template},
    referrals::{normalize_referral_code, referral_registry},
    whatsapp::types::{Button, ButtonReplyPayload},
};

use super::{advisor, customer_data, menu};

const REVIEW_CONTINUE: &str = "continue_review_checkout";
const REVIEW_CHANGE: &str = "change_review_checkout";
const REFERRAL_HAS_CODE: &str = "referral_has_code";
const REFERRAL_SKIP: &str = "referral_skip";
const REFERRAL_RETRY: &str = "referral_retry";
const CASH_ON_DELIVERY: &str = "cash_on_delivery";
const PAY_NOW: &str = "pay_now";
const CANCEL_ORDER: &str = "cancel_order";
const CHANGE_PAYMENT_METHOD: &str = "change_payment_method";

pub fn handle_review_checkout(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(REVIEW_CONTINUE) => {
            context.payment_method = None;
            context.receipt_media_id = None;
            context.receipt_timer_started_at = None;
            context.receipt_timer_expired = false;
            context.editing_address = false;
            context.clear_referral_data();
            context.delivery_cost = None;
            context.total_final = None;

            let (state, actions) = advisor::start_order_advisor_flow(context);
            Ok((state, actions))
        }
        Some(REVIEW_CHANGE) => Ok((
            ConversationState::SelectCustomerDataField,
            customer_data::select_customer_data_field_actions(context),
        )),
        _ => Ok((
            ConversationState::ReviewCheckout,
            review_checkout_actions(context),
        )),
    }
}

pub fn handle_select_referral_option(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(REFERRAL_HAS_CODE) => {
            context.clear_referral_data();
            Ok((
                ConversationState::WaitReferralCode,
                referral_code_prompt_actions(&context.phone_number),
            ))
        }
        Some(REFERRAL_SKIP) => {
            clear_referral_and_restore_total(context);
            Ok((
                ConversationState::SelectPaymentMethod,
                select_payment_method_actions(&context.phone_number),
            ))
        }
        _ => Ok((
            ConversationState::SelectReferralOption,
            select_referral_option_actions(&context.phone_number),
        )),
    }
}

pub fn handle_wait_referral_code(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(REFERRAL_RETRY) => {
            return Ok((
                ConversationState::WaitReferralCode,
                referral_code_prompt_actions(&context.phone_number),
            ));
        }
        Some(REFERRAL_SKIP) => {
            clear_referral_and_restore_total(context);
            return Ok((
                ConversationState::SelectPaymentMethod,
                select_payment_method_actions(&context.phone_number),
            ));
        }
        _ => {}
    }

    match input {
        UserInput::TextMessage(text) => {
            let normalized = normalize_referral_code(text);
            if normalized.is_empty() || !referral_registry().contains(&normalized) {
                return Ok((
                    ConversationState::WaitReferralCode,
                    invalid_referral_code_actions(&context.phone_number),
                ));
            }

            let pedido = calcular_pedido(&context.items);
            let Some(referral) = calcular_referido(&pedido, &normalized) else {
                clear_referral_and_restore_total(context);
                return Ok((
                    ConversationState::SelectPaymentMethod,
                    select_payment_method_actions(&context.phone_number),
                ));
            };

            apply_referral_to_context(context, &referral);

            let mut actions = vec![
                BotAction::UpsertDraftOrder {
                    status: "draft_payment".to_string(),
                },
                BotAction::SendText {
                    to: context.phone_number.clone(),
                    body: render_template(
                        &client_messages().checkout.referral_applied_text_template,
                        &[
                            ("code", &referral.code),
                            ("discount", &format_currency(referral.total_client_discount)),
                        ],
                    ),
                },
                BotAction::SendText {
                    to: context.phone_number.clone(),
                    body: render_payment_ready_confirmation(context),
                },
            ];
            actions.extend(select_payment_method_actions(&context.phone_number));

            Ok((ConversationState::SelectPaymentMethod, actions))
        }
        _ => Ok((
            ConversationState::WaitReferralCode,
            retry_actions(
                &context.phone_number,
                &client_messages().checkout.referral_code_non_text,
                referral_code_prompt_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn handle_select_payment_method(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(CASH_ON_DELIVERY) => {
            context.payment_method = Some(CASH_ON_DELIVERY.to_string());
            context.receipt_media_id = None;
            context.receipt_timer_started_at = None;
            context.receipt_timer_expired = false;

            let mut actions = vec![BotAction::UpsertDraftOrder {
                status: "confirmed".to_string(),
            }];
            actions.extend(advisor::final_order_packet_actions(context, None));

            Ok(complete_order_transition(context, "confirmed", actions))
        }
        Some(PAY_NOW) => {
            context.payment_method = Some("transfer".to_string());
            context.receipt_media_id = None;
            context.receipt_timer_started_at = Some(chrono::Utc::now());
            context.receipt_timer_expired = false;

            Ok((
                ConversationState::WaitReceipt,
                wait_receipt_entry_actions(context),
            ))
        }
        _ => Ok((
            ConversationState::SelectPaymentMethod,
            select_payment_method_actions(&context.phone_number),
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
                    ConversationState::SelectPaymentMethod,
                    select_payment_method_actions(&context.phone_number),
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

            let mut actions = vec![
                BotAction::CancelTimer {
                    timer_type: TimerType::ReceiptUpload,
                    phone: context.phone_number.clone(),
                },
                BotAction::UpsertDraftOrder {
                    status: "confirmed".to_string(),
                },
            ];
            actions.extend(advisor::final_order_packet_actions(context, Some(media_id)));
            actions.extend(final_confirmation_actions(context));

            Ok((ConversationState::MainMenu, actions))
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

pub fn handle_wait_advisor_response(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    advisor::handle_client_waiting_state(&ConversationState::WaitAdvisorResponse, input, context)
}

pub fn handle_order_complete(context: &mut ConversationContext) -> TransitionResult {
    let mut actions = vec![BotAction::ResetConversation {
        phone: context.phone_number.clone(),
    }];
    actions.extend(menu::main_menu_actions(&context.phone_number));

    Ok((ConversationState::MainMenu, actions))
}

pub fn review_checkout_actions(context: &ConversationContext) -> Vec<BotAction> {
    let messages = &client_messages().checkout;
    let pedido = calcular_pedido(&context.items);

    vec![
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: render_summary(context, &pedido),
        },
        BotAction::SendButtons {
            to: context.phone_number.clone(),
            body: messages.review_buttons_body.clone(),
            buttons: vec![
                reply_button(REVIEW_CONTINUE, &messages.review_continue_button),
                reply_button(REVIEW_CHANGE, &messages.review_change_button),
            ],
        },
    ]
}

pub fn payment_entry_state_and_actions(
    context: &ConversationContext,
) -> (ConversationState, Vec<BotAction>) {
    let pedido = calcular_pedido(&context.items);
    let mut actions = vec![BotAction::SendText {
        to: context.phone_number.clone(),
        body: render_payment_ready_confirmation(context),
    }];

    if context.referral_code.is_none() && has_wholesale_bucket(&pedido) {
        actions.extend(select_referral_option_actions(&context.phone_number));
        return (ConversationState::SelectReferralOption, actions);
    }

    actions.extend(select_payment_method_actions(&context.phone_number));
    (ConversationState::SelectPaymentMethod, actions)
}

pub fn select_referral_option_actions(phone: &str) -> Vec<BotAction> {
    let messages = &client_messages().checkout;

    vec![BotAction::SendButtons {
        to: phone.to_string(),
        body: messages.referral_prompt_body.clone(),
        buttons: vec![
            reply_button(REFERRAL_HAS_CODE, &messages.referral_has_code_button),
            reply_button(REFERRAL_SKIP, &messages.referral_skip_button),
        ],
    }]
}

pub fn referral_code_prompt_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages().checkout.referral_code_prompt.clone(),
    }]
}

pub fn select_payment_method_actions(phone: &str) -> Vec<BotAction> {
    let messages = &client_messages().checkout;

    vec![BotAction::SendButtons {
        to: phone.to_string(),
        body: messages.payment_buttons_body.clone(),
        buttons: vec![
            reply_button(CASH_ON_DELIVERY, &messages.cash_on_delivery_title),
            reply_button(PAY_NOW, &messages.pay_now_title),
        ],
    }]
}

pub fn confirm_address_actions(context: &ConversationContext) -> Vec<BotAction> {
    customer_data::confirm_customer_data_actions(context)
}

pub fn change_address_prompt_actions(phone: &str) -> Vec<BotAction> {
    customer_data::edit_customer_address_actions(phone)
}

pub fn render_summary(context: &ConversationContext, pedido: &PedidoCalculado) -> String {
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

pub fn render_items(items: &[ItemCalculated]) -> String {
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

pub fn render_payment_ready_confirmation(context: &ConversationContext) -> String {
    let pedido = calcular_pedido(&context.items);
    let delivery_cost = context.delivery_cost.unwrap_or_default();
    let total_final = context.total_final.unwrap_or_default();
    let discount_line = render_payment_ready_discount_line(context, &pedido);

    if context.delivery_type.as_deref() == Some("scheduled") {
        return render_template(
            &client_messages()
                .advisor_customer
                .scheduled_payment_ready_template,
            &[
                (
                    "date",
                    context.scheduled_date.as_deref().unwrap_or("pendiente"),
                ),
                (
                    "time",
                    context.scheduled_time.as_deref().unwrap_or("pendiente"),
                ),
                ("subtotal", &format_currency(pedido.total_estimado)),
                ("discount_line", &discount_line),
                (
                    "delivery_cost",
                    &format_currency(u32::try_from(delivery_cost).unwrap_or_default()),
                ),
                (
                    "total_final",
                    &format_currency(u32::try_from(total_final).unwrap_or_default()),
                ),
            ],
        );
    }

    render_template(
        &client_messages().advisor_customer.confirmed_order_template,
        &[
            ("subtotal", &format_currency(pedido.total_estimado)),
            ("discount_line", &discount_line),
            (
                "delivery_cost",
                &format_currency(u32::try_from(delivery_cost).unwrap_or_default()),
            ),
            (
                "total_final",
                &format_currency(u32::try_from(total_final).unwrap_or_default()),
            ),
            (
                "address",
                context.delivery_address.as_deref().unwrap_or("pendiente"),
            ),
        ],
    )
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

fn complete_order_transition(
    context: &mut ConversationContext,
    status: &str,
    mut actions: Vec<BotAction>,
) -> (ConversationState, Vec<BotAction>) {
    actions.extend(final_confirmation_actions(context));
    let _ = status;
    (ConversationState::MainMenu, actions)
}

fn final_confirmation_actions(context: &ConversationContext) -> Vec<BotAction> {
    let mut actions = vec![BotAction::SendText {
        to: context.phone_number.clone(),
        body: client_messages().checkout.final_order_success_text.clone(),
    }];
    actions.push(BotAction::ResetConversation {
        phone: context.phone_number.clone(),
    });
    actions.extend(menu::main_menu_actions(&context.phone_number));
    actions
}

fn cancel_order_transition(
    context: &mut ConversationContext,
) -> (ConversationState, Vec<BotAction>) {
    let cancel_action = context
        .current_order_id
        .map(|order_id| BotAction::CancelCurrentOrder { order_id });

    context.items.clear();
    context.payment_method = None;
    context.clear_referral_data();
    context.delivery_cost = None;
    context.total_final = None;
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

fn selection_id(input: &UserInput) -> Option<String> {
    match input {
        UserInput::ButtonPress(id) | UserInput::ListSelection(id) => Some(id.clone()),
        _ => None,
    }
}

fn render_payment_ready_discount_line(
    context: &ConversationContext,
    pedido: &PedidoCalculado,
) -> String {
    let Some(referral) = referral_from_context(context, pedido) else {
        return String::new();
    };

    format!(
        "Descuento referido: {}\n",
        format_currency(referral.total_client_discount)
    )
}

fn invalid_referral_code_actions(phone: &str) -> Vec<BotAction> {
    let messages = &client_messages().checkout;
    vec![BotAction::SendButtons {
        to: phone.to_string(),
        body: messages.referral_invalid_body.clone(),
        buttons: vec![
            reply_button(REFERRAL_RETRY, &messages.referral_retry_button),
            reply_button(REFERRAL_SKIP, &messages.referral_skip_button),
        ],
    }]
}

fn apply_referral_to_context(context: &mut ConversationContext, referral: &ReferralApplied) {
    context.referral_code = Some(referral.code.clone());
    context.referral_discount_total =
        Some(i32::try_from(referral.total_client_discount).unwrap_or(i32::MAX));
    context.ambassador_commission_total =
        Some(i32::try_from(referral.total_ambassador_commission).unwrap_or(i32::MAX));

    let delivery_cost = context.delivery_cost.unwrap_or_default();
    let subtotal_after_discount =
        i32::try_from(referral.subtotal_after_discount).unwrap_or(i32::MAX);
    context.total_final = Some(subtotal_after_discount.saturating_add(delivery_cost));
}

fn clear_referral_and_restore_total(context: &mut ConversationContext) {
    let pedido = calcular_pedido(&context.items);
    context.clear_referral_data();
    let delivery_cost = context.delivery_cost.unwrap_or_default();
    let subtotal = i32::try_from(pedido.total_estimado).unwrap_or(i32::MAX);
    context.total_final = Some(subtotal.saturating_add(delivery_cost));
}

fn referral_from_context(
    context: &ConversationContext,
    pedido: &PedidoCalculado,
) -> Option<ReferralApplied> {
    context
        .referral_code
        .as_deref()
        .and_then(|code| calcular_referido(pedido, code))
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

fn retry_actions(phone: &str, message: &str, mut actions: Vec<BotAction>) -> Vec<BotAction> {
    let mut all = vec![BotAction::SendText {
        to: phone.to_string(),
        body: message.to_string(),
    }];
    all.append(&mut actions);
    all
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
    use crate::bot::state_machine::{BotAction, ConversationContext, ConversationState, UserInput};

    use super::{
        handle_review_checkout, handle_select_payment_method, handle_select_referral_option,
        handle_wait_receipt, handle_wait_referral_code, payment_entry_state_and_actions,
        render_payment_ready_confirmation,
    };

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
            customer_review_scope: Some("checkout_review".to_string()),
            payment_method: None,
            referral_code: None,
            referral_discount_total: None,
            ambassador_commission_total: None,
            delivery_cost: Some(5000),
            total_final: Some(17000),
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

    fn wholesale_context() -> ConversationContext {
        let mut context = context();
        context.items = vec![crate::db::models::OrderItemData {
            flavor: "maracuya".to_string(),
            has_liquor: false,
            quantity: 20,
        }];
        context.delivery_cost = Some(5000);
        context.total_final = Some(101000);
        context
    }

    #[test]
    fn review_checkout_continue_moves_to_advisor_flow() {
        let mut context = context();

        let (state, actions) = handle_review_checkout(
            &UserInput::ButtonPress("continue_review_checkout".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::AskDeliveryCost);
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::FinalizeCurrentOrder { status }
            if status == "pending_advisor"
        )));
    }

    #[test]
    fn pay_now_moves_to_wait_receipt() {
        let mut context = context();

        let (state, actions) = handle_select_payment_method(
            &UserInput::ButtonPress("pay_now".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::WaitReceipt);
        assert_eq!(context.payment_method.as_deref(), Some("transfer"));
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::StartTimer { .. }
        )));
    }

    #[test]
    fn payment_entry_sends_wholesale_orders_to_referral_step_first() {
        let context = wholesale_context();

        let (state, actions) = payment_entry_state_and_actions(&context);

        assert_eq!(state, ConversationState::SelectReferralOption);
        assert!(actions.iter().any(|action| matches!(
            action,
            BotAction::SendButtons { body, .. }
                if body == &crate::messages::client_messages().checkout.referral_prompt_body
        )));
    }

    #[test]
    fn referral_skip_moves_directly_to_payment_buttons() {
        let mut context = wholesale_context();

        let (state, actions) = handle_select_referral_option(
            &UserInput::ButtonPress("referral_skip".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectPaymentMethod);
        assert_eq!(context.referral_code, None);
        assert!(actions.iter().any(|action| matches!(
            action,
            BotAction::SendButtons { buttons, .. }
                if buttons.iter().any(|button| button.reply.id == "pay_now")
        )));
    }

    #[test]
    fn valid_referral_code_is_normalized_and_updates_totals() {
        let mut context = wholesale_context();

        let (state, actions) = handle_wait_referral_code(
            &UserInput::TextMessage("  TRABIX-AMB15 ".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectPaymentMethod);
        assert_eq!(context.referral_code.as_deref(), Some("trabix-amb15"));
        assert_eq!(context.referral_discount_total, Some(9600));
        assert_eq!(context.ambassador_commission_total, Some(12960));
        assert_eq!(context.total_final, Some(91400));
        assert!(actions.iter().any(|action| matches!(
            action,
            BotAction::UpsertDraftOrder { status } if status == "draft_payment"
        )));
        assert!(actions.iter().any(|action| matches!(
            action,
            BotAction::SendText { body, .. }
                if body.contains("trabix-amb15") && body.contains("$9.600")
        )));
    }

    #[test]
    fn invalid_referral_code_keeps_customer_in_retry_state() {
        let mut context = wholesale_context();

        let (state, actions) = handle_wait_referral_code(
            &UserInput::TextMessage("desconocido".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::WaitReferralCode);
        assert_eq!(context.referral_code, None);
        assert!(actions.iter().any(|action| matches!(
            action,
            BotAction::SendButtons { body, buttons, .. }
                if body == &crate::messages::client_messages().checkout.referral_invalid_body
                    && buttons.iter().any(|button| button.reply.id == "referral_retry")
                    && buttons.iter().any(|button| button.reply.id == "referral_skip")
        )));
    }

    #[test]
    fn payment_ready_confirmation_renders_referral_discount_in_totals_when_present() {
        let mut context = wholesale_context();
        context.referral_code = Some("trabix-amb15".to_string());
        context.referral_discount_total = Some(9600);
        context.ambassador_commission_total = Some(12960);
        context.total_final = Some(91400);

        let rendered = render_payment_ready_confirmation(&context);

        assert!(rendered.contains("Descuento referido: $9.600"));
        assert!(rendered.contains(
            "Subtotal: $96.000\nDescuento referido: $9.600\nDomicilio: $5.000\nTotal final: $91.400"
        ));
        assert!(!rendered.contains("Código aplicado:"));
        assert!(!rendered.contains("Subtotal con descuento:"));
    }

    #[test]
    fn scheduled_payment_ready_confirmation_renders_referral_discount_in_totals() {
        let mut context = wholesale_context();
        context.delivery_type = Some("scheduled".to_string());
        context.scheduled_date = Some("2030-12-24".to_string());
        context.scheduled_time = Some("4:00 pm".to_string());
        context.referral_code = Some("trabix-amb15".to_string());
        context.referral_discount_total = Some(9600);
        context.ambassador_commission_total = Some(12960);
        context.total_final = Some(91400);

        let rendered = render_payment_ready_confirmation(&context);

        assert!(rendered.contains("Fecha de referencia: 2030-12-24"));
        assert!(rendered.contains("Hora acordada: 4:00 pm"));
        assert!(rendered.contains(
            "Subtotal: $96.000\nDescuento referido: $9.600\nDomicilio: $5.000\nTotal final: $91.400"
        ));
    }

    #[test]
    fn cash_on_delivery_sends_final_advisor_packet() {
        let mut context = context();
        context.delivery_type = Some("scheduled".to_string());
        context.scheduled_date = Some("2030-12-24".to_string());
        context.scheduled_time = Some("4:00 pm".to_string());

        let (state, actions) = handle_select_payment_method(
            &UserInput::ButtonPress("cash_on_delivery".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::MainMenu);
        assert_eq!(context.payment_method.as_deref(), Some("cash_on_delivery"));
        assert!(actions.iter().any(|action| matches!(
            action,
            BotAction::SendText { to, body }
                if to == "573009999999"
                    && body.contains("Cliente: Ana")
                    && body.contains("Teléfono: 3001234567")
                    && body.contains("Dirección: Cra 15 #20-30 Armenia")
                    && body.contains("Pago: Contra entrega")
                    && body.contains("Domicilio: $5.000")
                    && body.contains("Total final: $17.000")
        )));
        assert!(actions.iter().any(|action| matches!(
            action,
            BotAction::SendText { to, body }
                if to == "573009999999"
                    && body.contains("Pedido [...4567] confirmado. Método de pago final: contra entrega.")
        )));
    }

    #[test]
    fn wait_receipt_image_sends_final_advisor_packet_and_receipt() {
        let mut context = context();
        context.delivery_type = Some("scheduled".to_string());
        context.scheduled_date = Some("2030-12-24".to_string());
        context.scheduled_time = Some("4:00 pm".to_string());
        context.payment_method = Some("transfer".to_string());

        let (state, actions) = handle_wait_receipt(
            &UserInput::ImageMessage("media-1".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::MainMenu);
        assert_eq!(context.receipt_media_id.as_deref(), Some("media-1"));
        assert!(actions.iter().any(|action| matches!(
            action,
            BotAction::SendText { to, body }
                if to == "573009999999"
                    && body.contains("Cliente: Ana")
                    && body.contains("Entrega: Programada")
                    && body.contains("Pago: Pago por transferencia")
                    && body.contains("Domicilio: $5.000")
                    && body.contains("Total final: $17.000")
        )));
        assert!(actions.iter().any(|action| matches!(
            action,
            BotAction::SendText { to, body }
                if to == "573009999999"
                    && body.contains("Pago registrado por transferencia")
        )));
        assert!(actions.iter().any(|action| matches!(
            action,
            BotAction::SendImage { to, media_id, .. }
                if to == "573009999999" && media_id == "media-1"
        )));
    }

    #[test]
    fn wait_receipt_after_timeout_returns_to_payment_buttons() {
        let mut context = context();
        context.payment_method = Some("transfer".to_string());
        context.receipt_timer_expired = true;

        let (state, _) = handle_wait_receipt(
            &UserInput::ButtonPress("change_payment_method".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectPaymentMethod);
        assert_eq!(context.payment_method, None);
    }
}
