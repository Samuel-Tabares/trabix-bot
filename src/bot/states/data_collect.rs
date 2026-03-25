use crate::bot::{
    state_machine::{
        BotAction, ConversationContext, ConversationState, TransitionResult, UserInput,
    },
    states::{customer_data, order},
};
use crate::messages::client_messages;

pub fn handle_collect_name(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match validate_name(text) {
            Ok(name) => {
                context.customer_name = Some(name);
                Ok(customer_data::next_order_data_state(context))
            }
            Err(message) => Ok((
                ConversationState::CollectName,
                retry_actions(
                    &context.phone_number,
                    &message,
                    collect_name_actions(&context.phone_number),
                ),
            )),
        },
        _ => Ok((
            ConversationState::CollectName,
            retry_actions(
                &context.phone_number,
                &client_messages().data_collect.retry_name_non_text,
                collect_name_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn handle_collect_phone(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match validate_phone(text) {
            Ok(phone) => {
                context.customer_phone = Some(phone);
                Ok(customer_data::next_order_data_state(context))
            }
            Err(message) => Ok((
                ConversationState::CollectPhone,
                retry_actions(
                    &context.phone_number,
                    &message,
                    collect_phone_actions(&context.phone_number),
                ),
            )),
        },
        _ => Ok((
            ConversationState::CollectPhone,
            retry_actions(
                &context.phone_number,
                &client_messages().data_collect.retry_phone_non_text,
                collect_phone_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn handle_collect_address(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match validate_address(text) {
            Ok(address) => {
                context.delivery_address = Some(address);
                Ok((
                    ConversationState::SelectType,
                    order::select_type_actions(&context.phone_number),
                ))
            }
            Err(message) => Ok((
                ConversationState::CollectAddress,
                retry_actions(
                    &context.phone_number,
                    &message,
                    collect_address_actions(&context.phone_number),
                ),
            )),
        },
        _ => Ok((
            ConversationState::CollectAddress,
            retry_actions(
                &context.phone_number,
                &client_messages().data_collect.retry_address_non_text,
                collect_address_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn collect_name_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages().data_collect.ask_name.clone(),
    }]
}

pub fn collect_phone_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages().data_collect.ask_phone.clone(),
    }]
}

pub fn collect_address_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages().data_collect.ask_address.clone(),
    }]
}

pub fn validate_name(input: &str) -> Result<String, String> {
    let normalized = collapse_spaces(input);
    let length = normalized.chars().count();
    if !(2..=80).contains(&length) {
        return Err(client_messages().data_collect.name_length_error.clone());
    }

    Ok(normalized)
}

pub fn validate_phone(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if !trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(client_messages().data_collect.phone_digits_error.clone());
    }
    if !(7..=15).contains(&trimmed.len()) {
        return Err(client_messages().data_collect.phone_length_error.clone());
    }

    Ok(trimmed.to_string())
}

pub fn validate_address(input: &str) -> Result<String, String> {
    let normalized = collapse_spaces(input);
    let length = normalized.chars().count();
    if !(5..=160).contains(&length) {
        return Err(client_messages().data_collect.address_length_error.clone());
    }

    Ok(normalized)
}

fn collapse_spaces(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn retry_actions(phone: &str, message: &str, mut actions: Vec<BotAction>) -> Vec<BotAction> {
    let mut all = vec![BotAction::SendText {
        to: phone.to_string(),
        body: message.to_string(),
    }];
    all.append(&mut actions);
    all
}

#[cfg(test)]
mod tests {
    use crate::bot::state_machine::{ConversationContext, ConversationState, UserInput};

    use super::{
        handle_collect_address, handle_collect_name, handle_collect_phone, validate_address,
        validate_name, validate_phone,
    };

    fn context() -> ConversationContext {
        ConversationContext {
            phone_number: "573001234567".to_string(),
            advisor_phone: "573009999999".to_string(),
            customer_name: None,
            customer_phone: None,
            delivery_address: None,
            items: Vec::new(),
            delivery_type: None,
            scheduled_date: None,
            scheduled_time: None,
            customer_review_scope: None,
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
    fn validates_name() {
        assert_eq!(validate_name("  Ana   Maria ").unwrap(), "Ana Maria");
    }

    #[test]
    fn validates_phone() {
        assert_eq!(validate_phone("3001234567").unwrap(), "3001234567");
    }

    #[test]
    fn validates_address() {
        assert_eq!(
            validate_address(" Cra 15   #20-30 Armenia ").unwrap(),
            "Cra 15 #20-30 Armenia"
        );
    }

    #[test]
    fn collect_name_advances_to_phone() {
        let mut context = context();
        let (state, _) = handle_collect_name(
            &UserInput::TextMessage("Ana Maria".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::CollectPhone);
        assert_eq!(context.customer_name.as_deref(), Some("Ana Maria"));
    }

    #[test]
    fn collect_phone_advances_to_address() {
        let mut context = context();
        context.customer_name = Some("Ana Maria".to_string());
        let (state, _) = handle_collect_phone(
            &UserInput::TextMessage("3001234567".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::CollectAddress);
        assert_eq!(context.customer_phone.as_deref(), Some("3001234567"));
    }

    #[test]
    fn collect_address_advances_to_select_type() {
        let mut context = context();
        let (state, _) = handle_collect_address(
            &UserInput::TextMessage("Cra 15 #20-30 Armenia".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectType);
        assert_eq!(
            context.delivery_address.as_deref(),
            Some("Cra 15 #20-30 Armenia")
        );
    }
}
