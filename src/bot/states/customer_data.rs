use crate::{
    bot::state_machine::{
        BotAction, ConversationContext, ConversationState, TransitionResult, UserInput,
    },
    messages::{client_messages, render_template},
    whatsapp::types::{Button, ButtonReplyPayload},
};

use super::{advisor, data_collect};

pub const REVIEW_SCOPE_ORDER: &str = "order_checkout";
pub const REVIEW_SCOPE_ADVISOR: &str = "advisor_contact";

const CONTINUE_CUSTOMER_DATA: &str = "continue_customer_data";
const CHANGE_CUSTOMER_DATA: &str = "change_customer_data";
const EDIT_CUSTOMER_NAME: &str = "edit_customer_name";
const EDIT_CUSTOMER_PHONE: &str = "edit_customer_phone";
const EDIT_CUSTOMER_ADDRESS: &str = "edit_customer_address";

pub fn next_order_data_state(
    context: &mut ConversationContext,
) -> (ConversationState, Vec<BotAction>) {
    context.customer_review_scope = Some(REVIEW_SCOPE_ORDER.to_string());

    if context.customer_name.is_none() {
        return (
            ConversationState::CollectName,
            data_collect::collect_name_actions(&context.phone_number),
        );
    }

    if context.customer_phone.is_none() {
        return (
            ConversationState::CollectPhone,
            data_collect::collect_phone_actions(&context.phone_number),
        );
    }

    if context.delivery_address.is_none() {
        return (
            ConversationState::CollectAddress,
            data_collect::collect_address_actions(&context.phone_number),
        );
    }

    enter_review_state(context)
}

pub fn next_contact_advisor_state(
    context: &mut ConversationContext,
) -> (ConversationState, Vec<BotAction>) {
    context.customer_review_scope = Some(REVIEW_SCOPE_ADVISOR.to_string());

    if context.customer_name.is_none() {
        return (
            ConversationState::ContactAdvisorName,
            advisor::contact_advisor_name_actions(&context.phone_number),
        );
    }

    if context.customer_phone.is_none() {
        return (
            ConversationState::ContactAdvisorPhone,
            advisor::contact_advisor_phone_actions(&context.phone_number),
        );
    }

    enter_review_state(context)
}

pub fn handle_confirm_customer_data(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(CONTINUE_CUSTOMER_DATA) => {
            let scope = review_scope(context).to_string();
            context.customer_review_scope = None;

            let (state, actions) = match scope.as_str() {
                REVIEW_SCOPE_ADVISOR => advisor::start_waiting_for_contact_advisor(context),
                _ => advisor::handoff_order_after_address_confirmation(context),
            };

            Ok((state, actions))
        }
        Some(CHANGE_CUSTOMER_DATA) => Ok((
            ConversationState::SelectCustomerDataField,
            select_customer_data_field_actions(context),
        )),
        _ => Ok((
            ConversationState::ConfirmCustomerData,
            confirm_customer_data_actions(context),
        )),
    }
}

pub fn handle_select_customer_data_field(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(EDIT_CUSTOMER_NAME) => Ok((
            ConversationState::EditCustomerName,
            edit_customer_name_actions(context),
        )),
        Some(EDIT_CUSTOMER_PHONE) => Ok((
            ConversationState::EditCustomerPhone,
            edit_customer_phone_actions(context),
        )),
        Some(EDIT_CUSTOMER_ADDRESS) if review_scope(context) == REVIEW_SCOPE_ORDER => Ok((
            ConversationState::EditCustomerAddress,
            edit_customer_address_actions(&context.phone_number),
        )),
        _ => Ok((
            ConversationState::SelectCustomerDataField,
            select_customer_data_field_actions(context),
        )),
    }
}

pub fn handle_edit_customer_name(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match data_collect::validate_name(text) {
            Ok(name) => {
                context.customer_name = Some(name);
                Ok(enter_review_state(context))
            }
            Err(message) => Ok((
                ConversationState::EditCustomerName,
                retry_actions(
                    &context.phone_number,
                    &message,
                    edit_customer_name_actions(context),
                ),
            )),
        },
        _ => Ok((
            ConversationState::EditCustomerName,
            retry_actions(
                &context.phone_number,
                name_non_text_retry(context),
                edit_customer_name_actions(context),
            ),
        )),
    }
}

pub fn handle_edit_customer_phone(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match data_collect::validate_phone(text) {
            Ok(phone) => {
                context.customer_phone = Some(phone);
                Ok(enter_review_state(context))
            }
            Err(message) => Ok((
                ConversationState::EditCustomerPhone,
                retry_actions(
                    &context.phone_number,
                    &message,
                    edit_customer_phone_actions(context),
                ),
            )),
        },
        _ => Ok((
            ConversationState::EditCustomerPhone,
            retry_actions(
                &context.phone_number,
                phone_non_text_retry(context),
                edit_customer_phone_actions(context),
            ),
        )),
    }
}

pub fn handle_edit_customer_address(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match data_collect::validate_address(text) {
            Ok(address) => {
                context.delivery_address = Some(address);
                context.editing_address = false;
                Ok(enter_review_state(context))
            }
            Err(message) => Ok((
                ConversationState::EditCustomerAddress,
                retry_actions(
                    &context.phone_number,
                    &message,
                    edit_customer_address_actions(&context.phone_number),
                ),
            )),
        },
        _ => Ok((
            ConversationState::EditCustomerAddress,
            retry_actions(
                &context.phone_number,
                &client_messages().checkout.change_address_non_text,
                edit_customer_address_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn confirm_customer_data_actions(context: &ConversationContext) -> Vec<BotAction> {
    let (body, continue_label, change_label) = if review_scope(context) == REVIEW_SCOPE_ADVISOR {
        let messages = &client_messages().advisor_customer;
        (
            render_template(
                &messages.confirm_contact_template,
                &[
                    (
                        "customer_name",
                        context.customer_name.as_deref().unwrap_or("pendiente"),
                    ),
                    (
                        "customer_phone",
                        context.customer_phone.as_deref().unwrap_or("pendiente"),
                    ),
                ],
            ),
            messages.confirm_contact_continue_button.clone(),
            messages.confirm_contact_change_button.clone(),
        )
    } else {
        let messages = &client_messages().checkout;
        (
            render_template(
                &messages.confirm_customer_template,
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
                ],
            ),
            messages.confirm_customer_continue_button.clone(),
            messages.confirm_customer_change_button.clone(),
        )
    };

    vec![BotAction::SendButtons {
        to: context.phone_number.clone(),
        body,
        buttons: vec![
            reply_button(CONTINUE_CUSTOMER_DATA, &continue_label),
            reply_button(CHANGE_CUSTOMER_DATA, &change_label),
        ],
    }]
}

pub fn select_customer_data_field_actions(context: &ConversationContext) -> Vec<BotAction> {
    if review_scope(context) == REVIEW_SCOPE_ADVISOR {
        let messages = &client_messages().advisor_customer;
        return vec![BotAction::SendButtons {
            to: context.phone_number.clone(),
            body: messages.change_contact_field_body.clone(),
            buttons: vec![
                reply_button(EDIT_CUSTOMER_NAME, &messages.change_name_button),
                reply_button(EDIT_CUSTOMER_PHONE, &messages.change_phone_button),
            ],
        }];
    }

    let messages = &client_messages().checkout;
    vec![BotAction::SendButtons {
        to: context.phone_number.clone(),
        body: messages.change_customer_field_body.clone(),
        buttons: vec![
            reply_button(EDIT_CUSTOMER_NAME, &messages.change_name_button),
            reply_button(EDIT_CUSTOMER_PHONE, &messages.change_phone_button),
            reply_button(EDIT_CUSTOMER_ADDRESS, &messages.change_address_button),
        ],
    }]
}

pub fn edit_customer_name_actions(context: &ConversationContext) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: context.phone_number.clone(),
        body: name_prompt(context).to_string(),
    }]
}

pub fn edit_customer_phone_actions(context: &ConversationContext) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: context.phone_number.clone(),
        body: phone_prompt(context).to_string(),
    }]
}

pub fn edit_customer_address_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages().checkout.change_address_prompt.clone(),
    }]
}

fn enter_review_state(context: &mut ConversationContext) -> (ConversationState, Vec<BotAction>) {
    context.editing_address = false;
    (
        ConversationState::ConfirmCustomerData,
        confirm_customer_data_actions(context),
    )
}

fn review_scope(context: &ConversationContext) -> &str {
    context
        .customer_review_scope
        .as_deref()
        .unwrap_or(REVIEW_SCOPE_ORDER)
}

fn name_prompt(context: &ConversationContext) -> &str {
    if review_scope(context) == REVIEW_SCOPE_ADVISOR {
        &client_messages().advisor_customer.contact_name_prompt
    } else {
        &client_messages().data_collect.ask_name
    }
}

fn phone_prompt(context: &ConversationContext) -> &str {
    if review_scope(context) == REVIEW_SCOPE_ADVISOR {
        &client_messages().advisor_customer.contact_phone_prompt
    } else {
        &client_messages().data_collect.ask_phone
    }
}

fn name_non_text_retry(context: &ConversationContext) -> &str {
    if review_scope(context) == REVIEW_SCOPE_ADVISOR {
        &client_messages()
            .advisor_customer
            .contact_name_retry_non_text
    } else {
        &client_messages().data_collect.retry_name_non_text
    }
}

fn phone_non_text_retry(context: &ConversationContext) -> &str {
    if review_scope(context) == REVIEW_SCOPE_ADVISOR {
        &client_messages()
            .advisor_customer
            .contact_phone_retry_non_text
    } else {
        &client_messages().data_collect.retry_phone_non_text
    }
}

fn selection_id(input: &UserInput) -> Option<String> {
    match input {
        UserInput::ButtonPress(id) | UserInput::ListSelection(id) => Some(id.clone()),
        UserInput::TextMessage(_) | UserInput::ImageMessage(_) => None,
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

fn reply_button(id: &str, title: &str) -> Button {
    Button {
        kind: "reply".to_string(),
        reply: ButtonReplyPayload {
            id: id.to_string(),
            title: title.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::bot::state_machine::{ConversationContext, ConversationState, UserInput};

    use super::{
        handle_confirm_customer_data, handle_edit_customer_address,
        handle_select_customer_data_field, next_contact_advisor_state, next_order_data_state,
        REVIEW_SCOPE_ADVISOR, REVIEW_SCOPE_ORDER,
    };

    fn context() -> ConversationContext {
        ConversationContext {
            phone_number: "573001234567".to_string(),
            advisor_phone: "573009999999".to_string(),
            customer_name: Some("Ana".to_string()),
            customer_phone: Some("3001234567".to_string()),
            delivery_address: Some("Cra 15 #20-30 Armenia".to_string()),
            items: Vec::new(),
            delivery_type: Some("immediate".to_string()),
            scheduled_date: None,
            scheduled_time: None,
            customer_review_scope: None,
            payment_method: Some("cash_on_delivery".to_string()),
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
            current_order_id: None,
            editing_address: false,
            receipt_timer_expired: false,
            pending_has_liquor: None,
            pending_flavor: None,
            conversation_abandon_started_at: None,
            conversation_abandon_reminder_sent: false,
        }
    }

    #[test]
    fn order_data_flow_skips_to_review_when_all_fields_exist() {
        let mut context = context();

        let (state, _) = next_order_data_state(&mut context);

        assert_eq!(state, ConversationState::ConfirmCustomerData);
        assert_eq!(
            context.customer_review_scope.as_deref(),
            Some(REVIEW_SCOPE_ORDER)
        );
    }

    #[test]
    fn advisor_contact_flow_skips_to_review_when_all_fields_exist() {
        let mut context = context();
        context.delivery_address = None;

        let (state, _) = next_contact_advisor_state(&mut context);

        assert_eq!(state, ConversationState::ConfirmCustomerData);
        assert_eq!(
            context.customer_review_scope.as_deref(),
            Some(REVIEW_SCOPE_ADVISOR)
        );
    }

    #[test]
    fn change_path_uses_field_selector() {
        let mut context = context();
        context.customer_review_scope = Some(REVIEW_SCOPE_ORDER.to_string());

        let (state, _) = handle_confirm_customer_data(
            &UserInput::ButtonPress("change_customer_data".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectCustomerDataField);
    }

    #[test]
    fn selecting_address_enters_address_edit_state_for_orders() {
        let mut context = context();
        context.customer_review_scope = Some(REVIEW_SCOPE_ORDER.to_string());

        let (state, _) = handle_select_customer_data_field(
            &UserInput::ButtonPress("edit_customer_address".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::EditCustomerAddress);
    }

    #[test]
    fn editing_address_returns_to_review() {
        let mut context = context();
        context.customer_review_scope = Some(REVIEW_SCOPE_ORDER.to_string());

        let (state, _) = handle_edit_customer_address(
            &UserInput::TextMessage("Calle 1 #2-3".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::ConfirmCustomerData);
        assert_eq!(context.delivery_address.as_deref(), Some("Calle 1 #2-3"));
    }
}
