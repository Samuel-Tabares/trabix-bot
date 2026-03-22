use std::time::Duration;

use crate::{
    bot::{
        state_machine::{
            BotAction, ConversationContext, ConversationState, TimerType, TransitionResult,
            UserInput,
        },
        states::advisor::{parse_advisor_button_id, AdvisorButtonAction},
    },
    messages::client_messages,
    whatsapp::types::{Button, ButtonReplyPayload},
};

const RELAY_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const ADVISOR_FINISH_PREFIX: &str = "advisor_finish_";

pub fn handle_relay_mode(input: &UserInput, context: &mut ConversationContext) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) if !text.trim().is_empty() => {
            context.relay_timer_started_at = Some(chrono::Utc::now());

            Ok((
                ConversationState::RelayMode,
                vec![
                    BotAction::BindAdvisorSession {
                        advisor_phone: context.advisor_phone.clone(),
                        target_phone: context.phone_number.clone(),
                    },
                    BotAction::RelayMessage {
                        from: context.phone_number.clone(),
                        to: context.advisor_phone.clone(),
                        body: format!(
                            "[CLIENTE {}]: {}",
                            phone_marker(&context.phone_number),
                            text.trim()
                        ),
                    },
                    BotAction::StartTimer {
                        timer_type: TimerType::RelayInactivity,
                        phone: context.phone_number.clone(),
                        duration: RELAY_TIMEOUT,
                    },
                    BotAction::SendButtons {
                        to: context.advisor_phone.clone(),
                        body: "Relay activo.".to_string(),
                        buttons: vec![finish_button(&context.phone_number)],
                    },
                ],
            ))
        }
        _ => Ok((
            ConversationState::RelayMode,
            vec![BotAction::SendText {
                to: context.phone_number.clone(),
                body: client_messages().relay_customer.relay_text_only.clone(),
            }],
        )),
    }
}

pub fn handle_relay_mode_advisor(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::ButtonPress(id) | UserInput::ListSelection(id) => {
            if matches!(
                parse_advisor_button_id(id),
                Some((AdvisorButtonAction::Finish, _))
            ) {
                return Ok((
                    ConversationState::OrderComplete,
                    close_relay_actions(context, false),
                ));
            }

            Ok((
                ConversationState::RelayMode,
                vec![BotAction::SendText {
                    to: context.advisor_phone.clone(),
                    body: "En relay solo puedes enviar texto o usar el botón Finalizar."
                        .to_string(),
                }],
            ))
        }
        UserInput::TextMessage(text) if !text.trim().is_empty() => {
            context.relay_timer_started_at = Some(chrono::Utc::now());

            Ok((
                ConversationState::RelayMode,
                vec![
                    BotAction::BindAdvisorSession {
                        advisor_phone: context.advisor_phone.clone(),
                        target_phone: context.phone_number.clone(),
                    },
                    BotAction::RelayMessage {
                        from: context.advisor_phone.clone(),
                        to: context.phone_number.clone(),
                        body: text.trim().to_string(),
                    },
                    BotAction::StartTimer {
                        timer_type: TimerType::RelayInactivity,
                        phone: context.phone_number.clone(),
                        duration: RELAY_TIMEOUT,
                    },
                ],
            ))
        }
        _ => Ok((
            ConversationState::RelayMode,
            vec![BotAction::SendText {
                to: context.advisor_phone.clone(),
                body: client_messages().relay_customer.relay_text_only.clone(),
            }],
        )),
    }
}

pub fn relay_timeout_actions(context: &ConversationContext) -> Vec<BotAction> {
    close_relay_actions(context, true)
}

fn close_relay_actions(context: &ConversationContext, by_timeout: bool) -> Vec<BotAction> {
    let client_message = if by_timeout {
        client_messages()
            .relay_customer
            .relay_closed_by_timeout
            .as_str()
    } else {
        client_messages()
            .relay_customer
            .relay_closed_manual
            .as_str()
    };

    let advisor_message = if by_timeout {
        format!(
            "Relay {} cerrado por inactividad.",
            phone_marker(&context.phone_number)
        )
    } else {
        format!("Relay {} finalizado.", phone_marker(&context.phone_number))
    };

    vec![
        BotAction::CancelTimer {
            timer_type: TimerType::RelayInactivity,
            phone: context.phone_number.clone(),
        },
        BotAction::ClearAdvisorSession {
            advisor_phone: context.advisor_phone.clone(),
        },
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: client_message.to_string(),
        },
        BotAction::SendText {
            to: context.advisor_phone.clone(),
            body: advisor_message,
        },
        BotAction::ResetConversation {
            phone: context.phone_number.clone(),
        },
    ]
}

fn finish_button(phone: &str) -> Button {
    Button {
        kind: "reply".to_string(),
        reply: ButtonReplyPayload {
            id: format!("{ADVISOR_FINISH_PREFIX}{phone}"),
            title: format!("Finalizar {}", phone_marker(phone)),
        },
    }
}

fn phone_marker(phone: &str) -> String {
    let suffix = if phone.len() >= 4 {
        &phone[phone.len() - 4..]
    } else {
        phone
    };
    format!("[...{suffix}]")
}

#[cfg(test)]
mod tests {
    use crate::bot::state_machine::{ConversationContext, ConversationState, UserInput};

    use super::{handle_relay_mode, handle_relay_mode_advisor};

    fn context() -> ConversationContext {
        ConversationContext {
            phone_number: "573001234567".to_string(),
            advisor_phone: "573009999999".to_string(),
            customer_name: Some("Ana".to_string()),
            customer_phone: Some("3001234567".to_string()),
            delivery_address: Some("Cra 15 #20-30 Armenia".to_string()),
            items: Vec::new(),
            delivery_type: Some("scheduled".to_string()),
            scheduled_date: Some("2030-12-24".to_string()),
            scheduled_time: Some("5:00 pm".to_string()),
            payment_method: Some("cash_on_delivery".to_string()),
            receipt_media_id: None,
            receipt_timer_started_at: None,
            advisor_target_phone: Some("573001234567".to_string()),
            advisor_timer_started_at: None,
            advisor_timer_expired: false,
            relay_timer_started_at: Some(chrono::Utc::now()),
            relay_kind: Some("wholesale_order".to_string()),
            advisor_proposed_hour: None,
            client_counter_hour: None,
            schedule_resume_target: None,
            current_order_id: Some(10),
            editing_address: false,
            receipt_timer_expired: false,
            pending_has_liquor: None,
            pending_flavor: None,
        }
    }

    #[test]
    fn relay_forwards_client_text_to_advisor() {
        let mut context = context();

        let (state, actions) = handle_relay_mode(
            &UserInput::TextMessage("Hola asesor".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::RelayMode);
        assert!(actions.iter().any(|action| matches!(action, crate::bot::state_machine::BotAction::RelayMessage { to, body, .. } if to == "573009999999" && body.contains("[CLIENTE [...4567]]: Hola asesor"))));
    }

    #[test]
    fn relay_rejects_non_text_input() {
        let mut context = context();

        let (state, _actions) = handle_relay_mode(
            &UserInput::ImageMessage("media-1".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::RelayMode);
    }

    #[test]
    fn advisor_can_finish_relay() {
        let mut context = context();

        let (state, actions) = handle_relay_mode_advisor(
            &UserInput::ButtonPress("advisor_finish_573001234567".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::OrderComplete);
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::ResetConversation { .. }
        )));
    }
}
