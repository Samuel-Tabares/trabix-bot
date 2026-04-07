use std::time::Duration;

use chrono::{FixedOffset, Utc};

use crate::{
    bot::{
        pricing::{calcular_pedido, ItemCalculated, PedidoCalculado},
        state_machine::{
            BotAction, ConversationContext, ConversationState, TimerType, TransitionResult,
            UserInput,
        },
        states::{checkout, customer_data, data_collect, menu, scheduling},
        timers::{ADVISOR_AUTO_CANNOT_TIMEOUT, ADVISOR_RESPONSE_TIMEOUT, ADVISOR_STUCK_TIMEOUT},
    },
    messages::{client_messages, render_template},
    whatsapp::types::{Button, ButtonReplyPayload},
};

const ADVISOR_CONFIRM_PREFIX: &str = "advisor_confirm_";
const ADVISOR_YES_HOUR_PREFIX: &str = "advisor_yes_hour_";
const ADVISOR_OTHER_HOUR_PREFIX: &str = "advisor_other_hour_";
const ADVISOR_TAKE_PREFIX: &str = "advisor_take_";
const ADVISOR_FINISH_PREFIX: &str = "advisor_finish_";
const ADVISOR_ATTEND_PREFIX: &str = "advisor_attend_";
const ADVISOR_UNAVAILABLE_PREFIX: &str = "advisor_unavailable_";

const ACCEPT_PROPOSED_HOUR: &str = "accept_proposed_hour";
const REJECT_PROPOSED_HOUR: &str = "reject_proposed_hour";
const TIMEOUT_SCHEDULE: &str = "advisor_timeout_schedule";
const TIMEOUT_RETRY: &str = "advisor_timeout_retry";
const TIMEOUT_MENU: &str = "advisor_timeout_menu";
const LEAVE_MESSAGE: &str = "leave_message";
const BACK_MAIN_MENU: &str = "back_main_menu";

const RELAY_KIND_WHOLESALE: &str = "wholesale_order";
pub const RELAY_KIND_CONTACT: &str = "contact_advisor";

const HOUR_MIN_LEN: usize = 1;
const HOUR_MAX_LEN: usize = 40;
const LEAVE_MESSAGE_MIN_LEN: usize = 2;
const LEAVE_MESSAGE_MAX_LEN: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvisorButtonAction {
    Confirm,
    YesHour,
    OtherHour,
    Take,
    Finish,
    Attend,
    Unavailable,
}

pub fn parse_advisor_button_id(button_id: &str) -> Option<(AdvisorButtonAction, String)> {
    [
        (ADVISOR_CONFIRM_PREFIX, AdvisorButtonAction::Confirm),
        (ADVISOR_YES_HOUR_PREFIX, AdvisorButtonAction::YesHour),
        (ADVISOR_OTHER_HOUR_PREFIX, AdvisorButtonAction::OtherHour),
        (ADVISOR_TAKE_PREFIX, AdvisorButtonAction::Take),
        (ADVISOR_FINISH_PREFIX, AdvisorButtonAction::Finish),
        (ADVISOR_ATTEND_PREFIX, AdvisorButtonAction::Attend),
        (ADVISOR_UNAVAILABLE_PREFIX, AdvisorButtonAction::Unavailable),
    ]
    .iter()
    .find_map(|(prefix, action)| {
        button_id
            .strip_prefix(prefix)
            .filter(|phone| !phone.is_empty())
            .map(|phone| (*action, phone.to_string()))
    })
}

pub fn start_contact_advisor(
    context: &mut ConversationContext,
) -> (ConversationState, Vec<BotAction>) {
    customer_data::next_contact_advisor_state(context)
}

pub fn start_order_advisor_flow(
    context: &mut ConversationContext,
) -> (ConversationState, Vec<BotAction>) {
    context.advisor_timer_started_at = Some(chrono::Utc::now());
    context.advisor_timer_expired = false;
    context.schedule_resume_target = None;
    context.relay_kind = None;
    context.relay_timer_started_at = None;
    context.payment_method = None;
    context.receipt_media_id = None;
    context.receipt_timer_started_at = None;
    context.receipt_timer_expired = false;

    let pedido = calcular_pedido(&context.items);
    (
        ConversationState::AskDeliveryCost,
        ask_delivery_cost_entry_actions(context, &pedido),
    )
}

pub fn resume_after_schedule_confirmation(
    context: &mut ConversationContext,
) -> (ConversationState, Vec<BotAction>) {
    context.delivery_type = Some("scheduled".to_string());
    context.advisor_timer_started_at = Some(chrono::Utc::now());
    context.advisor_timer_expired = false;

    let target = context
        .schedule_resume_target
        .clone()
        .unwrap_or_else(|| "wait_advisor_response".to_string());
    context.schedule_resume_target = None;
    context.relay_kind = None;

    let pedido = calcular_pedido(&context.items);
    match target.as_str() {
        "wait_advisor_mayor" => (
            ConversationState::WaitAdvisorMayor,
            wait_advisor_mayor_entry_actions(context, &pedido),
        ),
        _ => (
            ConversationState::AskDeliveryCost,
            ask_delivery_cost_entry_actions(context, &pedido),
        ),
    }
}

pub fn handle_contact_advisor_name(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match data_collect::validate_name(text) {
            Ok(name) => {
                context.customer_name = Some(name);
                let (state, actions) = customer_data::next_contact_advisor_state(context);
                Ok((state, actions))
            }
            Err(message) => Ok((
                ConversationState::ContactAdvisorName,
                retry_actions(
                    &context.phone_number,
                    &message,
                    contact_advisor_name_actions(&context.phone_number),
                ),
            )),
        },
        _ => Ok((
            ConversationState::ContactAdvisorName,
            retry_actions(
                &context.phone_number,
                &client_messages()
                    .advisor_customer
                    .contact_name_retry_non_text,
                contact_advisor_name_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn handle_contact_advisor_phone(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match data_collect::validate_phone(text) {
            Ok(phone) => {
                context.customer_phone = Some(phone);
                let (state, actions) = customer_data::next_contact_advisor_state(context);
                Ok((state, actions))
            }
            Err(message) => Ok((
                ConversationState::ContactAdvisorPhone,
                retry_actions(
                    &context.phone_number,
                    &message,
                    contact_advisor_phone_actions(&context.phone_number),
                ),
            )),
        },
        _ => Ok((
            ConversationState::ContactAdvisorPhone,
            retry_actions(
                &context.phone_number,
                &client_messages()
                    .advisor_customer
                    .contact_phone_retry_non_text,
                contact_advisor_phone_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn handle_wait_advisor_contact(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    if context.advisor_timer_expired {
        return match selection_id(input).as_deref() {
            Some(LEAVE_MESSAGE) => Ok((
                ConversationState::LeaveMessage,
                vec![BotAction::SendText {
                    to: context.phone_number.clone(),
                    body: client_messages()
                        .advisor_customer
                        .wait_contact_leave_message_prompt
                        .clone(),
                }],
            )),
            Some(BACK_MAIN_MENU) => Ok(reset_to_main_menu(context, false)),
            _ => Ok((
                ConversationState::WaitAdvisorContact,
                wait_advisor_contact_timeout_actions(&context.phone_number),
            )),
        };
    }

    Ok((
        ConversationState::WaitAdvisorContact,
        vec![BotAction::SendText {
            to: context.phone_number.clone(),
            body: client_messages()
                .advisor_customer
                .wait_contact_repeat_text
                .clone(),
        }],
    ))
}

pub fn handle_leave_message(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match validate_message(text) {
            Ok(message) => Ok((
                ConversationState::OrderComplete,
                vec![
                    BotAction::SendText {
                        to: context.advisor_phone.clone(),
                        body: render_left_message(context, &message),
                    },
                    BotAction::SendText {
                        to: context.phone_number.clone(),
                        body: client_messages()
                            .advisor_customer
                            .leave_message_success
                            .clone(),
                    },
                    BotAction::ResetConversation {
                        phone: context.phone_number.clone(),
                    },
                ],
            )),
            Err(message) => Ok((
                ConversationState::LeaveMessage,
                retry_actions(
                    &context.phone_number,
                    &message,
                    vec![BotAction::SendText {
                        to: context.phone_number.clone(),
                        body: client_messages()
                            .advisor_customer
                            .wait_contact_leave_message_prompt
                            .clone(),
                    }],
                ),
            )),
        },
        _ => Ok((
            ConversationState::LeaveMessage,
            vec![BotAction::SendText {
                to: context.phone_number.clone(),
                body: client_messages()
                    .advisor_customer
                    .leave_message_non_text
                    .clone(),
            }],
        )),
    }
}

pub fn handle_client_waiting_state(
    state: &ConversationState,
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match state {
        ConversationState::WaitAdvisorResponse => {
            handle_client_wait_advisor_response(input, context)
        }
        ConversationState::AskDeliveryCost => Ok((
            ConversationState::AskDeliveryCost,
            vec![BotAction::SendText {
                to: context.phone_number.clone(),
                body: client_messages()
                    .advisor_customer
                    .wait_delivery_cost_text
                    .clone(),
            }],
        )),
        ConversationState::NegotiateHour => Ok((
            ConversationState::NegotiateHour,
            vec![BotAction::SendText {
                to: context.phone_number.clone(),
                body: client_messages()
                    .advisor_customer
                    .wait_negotiate_hour_text
                    .clone(),
            }],
        )),
        ConversationState::OfferHourToClient { proposed_hour } => {
            handle_offer_hour_to_client(input, context, proposed_hour)
        }
        ConversationState::WaitClientHour => handle_wait_client_hour(input, context),
        ConversationState::WaitAdvisorHourDecision { .. } => Ok((
            ConversationState::WaitAdvisorHourDecision {
                client_hour: context
                    .client_counter_hour
                    .clone()
                    .unwrap_or_else(|| "pendiente".to_string()),
            },
            vec![BotAction::SendText {
                to: context.phone_number.clone(),
                body: client_messages()
                    .advisor_customer
                    .wait_advisor_hour_decision_text
                    .clone(),
            }],
        )),
        ConversationState::WaitAdvisorConfirmHour => Ok((
            ConversationState::WaitAdvisorConfirmHour,
            vec![BotAction::SendText {
                to: context.phone_number.clone(),
                body: client_messages()
                    .advisor_customer
                    .wait_advisor_confirm_text
                    .clone(),
            }],
        )),
        ConversationState::WaitAdvisorMayor => handle_client_wait_advisor_mayor(input, context),
        _ => Ok((
            state.clone(),
            vec![BotAction::SendText {
                to: context.phone_number.clone(),
                body: client_messages().advisor_customer.wait_general_text.clone(),
            }],
        )),
    }
}

pub fn handle_advisor_wait_advisor_response(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    let Some(AdvisorButtonAction::Confirm) = advisor_button_action(input) else {
        return Ok((
            ConversationState::WaitAdvisorResponse,
            vec![BotAction::SendText {
                to: context.advisor_phone.clone(),
                body: format!(
                    "Usa uno de los botones del caso {} para continuar.",
                    phone_marker(&context.phone_number)
                ),
            }],
        ));
    };

    context.advisor_timer_started_at = None;
    context.advisor_timer_expired = false;

    transition_to_payment_selection(context)
}

pub fn handle_advisor_ask_delivery_cost(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match parse_delivery_cost(text) {
            Ok(delivery_cost) => {
                context.advisor_timer_started_at = None;
                context.advisor_timer_expired = false;
                let pedido = calcular_pedido(&context.items);
                let total_final =
                    i32::try_from(pedido.total_estimado).unwrap_or(i32::MAX) + delivery_cost;
                context.delivery_cost = Some(delivery_cost);
                context.total_final = Some(total_final);

                if context.delivery_type.as_deref() == Some("immediate") {
                    context.advisor_timer_started_at = Some(chrono::Utc::now());

                    Ok((
                        ConversationState::WaitAdvisorResponse,
                        vec![
                            BotAction::CancelTimer {
                                timer_type: TimerType::AdvisorResponse,
                                phone: context.phone_number.clone(),
                            },
                            BotAction::UpdateCurrentOrderDeliveryCost {
                                delivery_cost,
                                total_final,
                                status: "pending_advisor".to_string(),
                            },
                            BotAction::SendText {
                                to: context.advisor_phone.clone(),
                                body: format!(
                                    "Perfecto. Ahora confirma disponibilidad para {}.",
                                    phone_marker(&context.phone_number)
                                ),
                            },
                            BotAction::SendButtons {
                                to: context.advisor_phone.clone(),
                                body: "Selecciona cómo deseas responder a este pedido.".to_string(),
                                buttons: advisor_order_buttons(context),
                            },
                            BotAction::SendText {
                                to: context.phone_number.clone(),
                                body: client_messages()
                                    .advisor_customer
                                    .availability_wait_text
                                    .clone(),
                            },
                            BotAction::StartTimer {
                                timer_type: TimerType::AdvisorResponse,
                                phone: context.phone_number.clone(),
                                duration: ADVISOR_AUTO_CANNOT_TIMEOUT,
                            },
                        ],
                    ))
                } else {
                    let mut actions = vec![
                        BotAction::CancelTimer {
                            timer_type: TimerType::AdvisorResponse,
                            phone: context.phone_number.clone(),
                        },
                        BotAction::UpdateCurrentOrderDeliveryCost {
                            delivery_cost,
                            total_final,
                            status: "draft_payment".to_string(),
                        },
                        BotAction::ClearAdvisorSession {
                            advisor_phone: context.advisor_phone.clone(),
                        },
                        BotAction::SendText {
                            to: context.advisor_phone.clone(),
                            body: format!(
                                "Pedido programado {} listo para que el cliente elija el pago.",
                                phone_marker(&context.phone_number)
                            ),
                        },
                        BotAction::SendText {
                            to: context.phone_number.clone(),
                            body: checkout::render_payment_ready_confirmation(context),
                        },
                    ];
                    actions.extend(checkout::select_payment_method_actions(
                        &context.phone_number,
                    ));

                    Ok((ConversationState::SelectPaymentMethod, actions))
                }
            }
            Err(message) => Ok((
                ConversationState::AskDeliveryCost,
                vec![BotAction::SendText {
                    to: context.advisor_phone.clone(),
                    body: message,
                }],
            )),
        },
        _ => Ok((
            ConversationState::AskDeliveryCost,
            vec![BotAction::SendText {
                to: context.advisor_phone.clone(),
                body: format!(
                    "Por favor envíe solo el valor numérico del domicilio para {} (ej: 5000).",
                    phone_marker(&context.phone_number)
                ),
            }],
        )),
    }
}

pub fn handle_advisor_negotiate_hour(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match validate_hour_text(text) {
            Ok(hour) => {
                context.advisor_timer_started_at = None;
                context.advisor_timer_expired = false;
                context.advisor_proposed_hour = Some(hour.clone());

                Ok((
                    ConversationState::OfferHourToClient {
                        proposed_hour: hour.clone(),
                    },
                    vec![
                        BotAction::CancelTimer {
                            timer_type: TimerType::AdvisorResponse,
                            phone: context.phone_number.clone(),
                        },
                        BotAction::ClearAdvisorSession {
                            advisor_phone: context.advisor_phone.clone(),
                        },
                        BotAction::SendText {
                            to: context.phone_number.clone(),
                            body: render_template(
                                &client_messages()
                                    .advisor_customer
                                    .proposed_hour_question_template,
                                &[("hour", &hour)],
                            ),
                        },
                        BotAction::SendButtons {
                            to: context.phone_number.clone(),
                            body: client_messages()
                                .advisor_customer
                                .proposed_hour_buttons_body
                                .clone(),
                            buttons: vec![
                                reply_button(
                                    ACCEPT_PROPOSED_HOUR,
                                    &client_messages().advisor_customer.accept_button,
                                ),
                                reply_button(
                                    REJECT_PROPOSED_HOUR,
                                    &client_messages().advisor_customer.reject_button,
                                ),
                            ],
                        },
                    ],
                ))
            }
            Err(message) => Ok((
                ConversationState::NegotiateHour,
                vec![BotAction::SendText {
                    to: context.advisor_phone.clone(),
                    body: message,
                }],
            )),
        },
        _ => Ok((
            ConversationState::NegotiateHour,
            vec![BotAction::SendText {
                to: context.advisor_phone.clone(),
                body: format!(
                    "Escribe una hora de referencia para el caso {}.",
                    phone_marker(&context.phone_number)
                ),
            }],
        )),
    }
}

pub fn handle_advisor_hour_decision(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match advisor_button_action(input) {
        Some(AdvisorButtonAction::YesHour) => transition_to_payment_selection(context),
        Some(AdvisorButtonAction::OtherHour) => {
            context.advisor_timer_started_at = Some(chrono::Utc::now());
            context.advisor_timer_expired = false;
            Ok((
                ConversationState::NegotiateHour,
                vec![
                    BotAction::BindAdvisorSession {
                        advisor_phone: context.advisor_phone.clone(),
                        target_phone: context.phone_number.clone(),
                    },
                    BotAction::SendText {
                        to: context.advisor_phone.clone(),
                        body: format!(
                            "Perfecto. Envíe otra hora para {}.",
                            phone_marker(&context.phone_number)
                        ),
                    },
                    BotAction::StartTimer {
                        timer_type: TimerType::AdvisorResponse,
                        phone: context.phone_number.clone(),
                        duration: ADVISOR_STUCK_TIMEOUT,
                    },
                ],
            ))
        }
        _ => Ok((
            ConversationState::WaitAdvisorHourDecision {
                client_hour: context
                    .client_counter_hour
                    .clone()
                    .unwrap_or_else(|| "pendiente".to_string()),
            },
            vec![BotAction::SendText {
                to: context.advisor_phone.clone(),
                body: format!(
                    "Usa los botones enviados para responder al caso {}.",
                    phone_marker(&context.phone_number)
                ),
            }],
        )),
    }
}

pub fn handle_advisor_confirm_hour(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match advisor_button_action(input) {
        Some(AdvisorButtonAction::Confirm) => transition_to_payment_selection(context),
        _ => Ok((
            ConversationState::WaitAdvisorConfirmHour,
            vec![BotAction::SendText {
                to: context.advisor_phone.clone(),
                body: format!(
                    "Usa el botón de confirmación del caso {} para continuar.",
                    phone_marker(&context.phone_number)
                ),
            }],
        )),
    }
}

pub fn handle_advisor_wait_advisor_mayor(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match advisor_button_action(input) {
        Some(AdvisorButtonAction::Take) => {
            context.advisor_timer_started_at = None;
            context.advisor_timer_expired = false;
            context.relay_kind = Some(RELAY_KIND_WHOLESALE.to_string());
            context.relay_timer_started_at = Some(chrono::Utc::now());

            Ok((
                ConversationState::RelayMode,
                relay_entry_actions(
                    context,
                    &client_messages().relay_customer.wholesale_connected_text,
                    "Tomaste el pedido al por mayor y el relay quedó activo.",
                    true,
                ),
            ))
        }
        _ => Ok((
            ConversationState::WaitAdvisorMayor,
            vec![BotAction::SendText {
                to: context.advisor_phone.clone(),
                body: format!(
                    "Usa el botón del caso {} para tomar el pedido.",
                    phone_marker(&context.phone_number)
                ),
            }],
        )),
    }
}

pub fn handle_advisor_wait_advisor_contact(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match advisor_button_action(input) {
        Some(AdvisorButtonAction::Attend) => {
            context.advisor_timer_started_at = None;
            context.advisor_timer_expired = false;
            context.relay_kind = Some(RELAY_KIND_CONTACT.to_string());
            context.relay_timer_started_at = Some(chrono::Utc::now());

            Ok((
                ConversationState::RelayMode,
                relay_entry_actions(
                    context,
                    &client_messages()
                        .relay_customer
                        .direct_contact_connected_text,
                    "Comenzó el relay con el cliente.",
                    false,
                ),
            ))
        }
        Some(AdvisorButtonAction::Unavailable) => {
            context.advisor_timer_started_at = None;
            context.advisor_timer_expired = true;

            Ok((
                ConversationState::WaitAdvisorContact,
                vec![
                    BotAction::CancelTimer {
                        timer_type: TimerType::AdvisorResponse,
                        phone: context.phone_number.clone(),
                    },
                    BotAction::ClearAdvisorSession {
                        advisor_phone: context.advisor_phone.clone(),
                    },
                    BotAction::SendText {
                        to: context.advisor_phone.clone(),
                        body: format!(
                            "Marcaste el caso {} como no disponible.",
                            phone_marker(&context.phone_number)
                        ),
                    },
                    BotAction::SendButtons {
                        to: context.phone_number.clone(),
                        body: client_messages()
                            .timers_customer
                            .contact_timeout_body
                            .clone(),
                        buttons: vec![
                            reply_button(
                                LEAVE_MESSAGE,
                                &client_messages()
                                    .timers_customer
                                    .contact_timeout_leave_message_button,
                            ),
                            reply_button(
                                BACK_MAIN_MENU,
                                &client_messages()
                                    .timers_customer
                                    .contact_timeout_menu_button,
                            ),
                        ],
                    },
                ],
            ))
        }
        _ => Ok((
            ConversationState::WaitAdvisorContact,
            vec![BotAction::SendText {
                to: context.advisor_phone.clone(),
                body: format!(
                    "Usa uno de los botones del caso {} para continuar.",
                    phone_marker(&context.phone_number)
                ),
            }],
        )),
    }
}

pub fn handle_advisor_unexpected_state(
    state: &ConversationState,
    context: &mut ConversationContext,
) -> TransitionResult {
    Ok((
        state.clone(),
        vec![
            BotAction::ClearAdvisorSession {
                advisor_phone: context.advisor_phone.clone(),
            },
            BotAction::SendText {
                to: context.advisor_phone.clone(),
                body: format!(
                    "El caso {} ya no espera una acción del asesor.",
                    phone_marker(&context.phone_number)
                ),
            },
        ],
    ))
}

pub fn advisor_timeout_actions(
    context: &ConversationContext,
    wait_state: ConversationState,
) -> Vec<BotAction> {
    let messages = &client_messages().timers_customer;
    vec![
        BotAction::ClearAdvisorSession {
            advisor_phone: context.advisor_phone.clone(),
        },
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: match wait_state {
                ConversationState::WaitAdvisorMayor => {
                    messages.advisor_timeout_wholesale_text.clone()
                }
                _ => messages.advisor_timeout_text.clone(),
            },
        },
        BotAction::SendButtons {
            to: context.phone_number.clone(),
            body: messages.advisor_timeout_buttons_body.clone(),
            buttons: vec![
                reply_button(TIMEOUT_SCHEDULE, &messages.advisor_timeout_schedule_button),
                reply_button(TIMEOUT_RETRY, &messages.advisor_timeout_retry_button),
                reply_button(TIMEOUT_MENU, &messages.advisor_timeout_menu_button),
            ],
        },
    ]
}

pub fn wait_advisor_contact_timeout_actions(phone: &str) -> Vec<BotAction> {
    let messages = &client_messages().timers_customer;
    vec![BotAction::SendButtons {
        to: phone.to_string(),
        body: messages.contact_timeout_body.clone(),
        buttons: vec![
            reply_button(
                LEAVE_MESSAGE,
                &messages.contact_timeout_leave_message_button,
            ),
            reply_button(BACK_MAIN_MENU, &messages.contact_timeout_menu_button),
        ],
    }]
}

pub fn contact_advisor_name_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages()
            .advisor_customer
            .contact_name_prompt
            .clone(),
    }]
}

pub fn contact_advisor_phone_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages()
            .advisor_customer
            .contact_phone_prompt
            .clone(),
    }]
}

pub fn leave_message_prompt_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages()
            .advisor_customer
            .wait_contact_leave_message_prompt
            .clone(),
    }]
}

pub fn offer_hour_to_client_actions(phone: &str, proposed_hour: &str) -> Vec<BotAction> {
    vec![
        BotAction::SendText {
            to: phone.to_string(),
            body: render_template(
                &client_messages()
                    .advisor_customer
                    .proposed_hour_question_template,
                &[("hour", proposed_hour)],
            ),
        },
        BotAction::SendButtons {
            to: phone.to_string(),
            body: client_messages()
                .advisor_customer
                .proposed_hour_buttons_body
                .clone(),
            buttons: vec![
                reply_button(
                    ACCEPT_PROPOSED_HOUR,
                    &client_messages().advisor_customer.accept_button,
                ),
                reply_button(
                    REJECT_PROPOSED_HOUR,
                    &client_messages().advisor_customer.reject_button,
                ),
            ],
        },
    ]
}

pub fn wait_client_hour_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages()
            .advisor_customer
            .client_hour_prompt
            .clone(),
    }]
}

fn handle_client_wait_advisor_response(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    let _ = input;
    if context.advisor_timer_expired {
        return transition_immediate_order_to_scheduled(context);
    }

    Ok((
        ConversationState::WaitAdvisorResponse,
        vec![BotAction::SendText {
            to: context.phone_number.clone(),
            body: client_messages()
                .advisor_customer
                .availability_wait_text
                .clone(),
        }],
    ))
}

fn handle_client_wait_advisor_mayor(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    if context.advisor_timer_expired {
        return handle_advisor_timeout_selection(
            input,
            context,
            ConversationState::WaitAdvisorMayor,
        );
    }

    Ok((
        ConversationState::WaitAdvisorMayor,
        vec![BotAction::SendText {
            to: context.phone_number.clone(),
            body: client_messages()
                .advisor_customer
                .wholesale_wait_text
                .clone(),
        }],
    ))
}

fn handle_advisor_timeout_selection(
    input: &UserInput,
    context: &mut ConversationContext,
    wait_state: ConversationState,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(TIMEOUT_RETRY) => {
            context.advisor_timer_started_at = Some(chrono::Utc::now());
            context.advisor_timer_expired = false;
            let pedido = calcular_pedido(&context.items);
            let actions = if wait_state == ConversationState::WaitAdvisorMayor {
                wait_advisor_mayor_entry_actions(context, &pedido)
            } else {
                wait_advisor_response_entry_actions(context, &pedido)
            };
            Ok((wait_state, actions))
        }
        Some(TIMEOUT_SCHEDULE) => {
            context.schedule_resume_target = Some(wait_state.as_storage_key().to_string());
            context.scheduled_date = None;
            context.scheduled_time = None;
            Ok((
                ConversationState::SelectDate,
                scheduling::select_date_actions(&context.phone_number),
            ))
        }
        Some(TIMEOUT_MENU) => Ok(reset_to_main_menu(context, true)),
        _ => Ok((
            wait_state.clone(),
            advisor_timeout_actions(context, wait_state),
        )),
    }
}

fn handle_offer_hour_to_client(
    input: &UserInput,
    context: &mut ConversationContext,
    proposed_hour: &str,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(ACCEPT_PROPOSED_HOUR) => {
            context.advisor_timer_started_at = Some(chrono::Utc::now());
            context.advisor_timer_expired = false;
            Ok((
                ConversationState::WaitAdvisorConfirmHour,
                vec![
                    BotAction::SendText {
                        to: context.advisor_phone.clone(),
                        body: format!(
                            "El cliente {} aceptó la hora {}. ¿Confirmas el pedido?",
                            phone_marker(&context.phone_number),
                            proposed_hour
                        ),
                    },
                    BotAction::SendButtons {
                        to: context.advisor_phone.clone(),
                        body: "Confirma el pedido para continuar.".to_string(),
                        buttons: vec![reply_button(
                            &advisor_button_id(ADVISOR_CONFIRM_PREFIX, &context.phone_number),
                            &advisor_title("Confirmar", &context.phone_number),
                        )],
                    },
                    BotAction::StartTimer {
                        timer_type: TimerType::AdvisorResponse,
                        phone: context.phone_number.clone(),
                        duration: ADVISOR_STUCK_TIMEOUT,
                    },
                ],
            ))
        }
        Some(REJECT_PROPOSED_HOUR) => Ok((
            ConversationState::WaitClientHour,
            vec![BotAction::SendText {
                to: context.phone_number.clone(),
                body: client_messages()
                    .advisor_customer
                    .client_hour_prompt
                    .clone(),
            }],
        )),
        _ => Ok((
            ConversationState::OfferHourToClient {
                proposed_hour: proposed_hour.to_string(),
            },
            vec![BotAction::SendButtons {
                to: context.phone_number.clone(),
                body: render_template(
                    &client_messages()
                        .advisor_customer
                        .proposed_hour_repeat_template,
                    &[("hour", proposed_hour)],
                ),
                buttons: vec![
                    reply_button(
                        ACCEPT_PROPOSED_HOUR,
                        &client_messages().advisor_customer.accept_button,
                    ),
                    reply_button(
                        REJECT_PROPOSED_HOUR,
                        &client_messages().advisor_customer.reject_button,
                    ),
                ],
            }],
        )),
    }
}

fn handle_wait_client_hour(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match validate_client_hour_text(text) {
            Ok(hour) => {
                context.advisor_timer_started_at = Some(chrono::Utc::now());
                context.advisor_timer_expired = false;
                context.client_counter_hour = Some(hour.clone());

                Ok((
                    ConversationState::WaitAdvisorHourDecision {
                        client_hour: hour.clone(),
                    },
                    vec![
                        BotAction::SendText {
                            to: context.advisor_phone.clone(),
                            body: format!(
                                "El cliente {} propone {}. ¿Puedes confirmar esa hora?",
                                phone_marker(&context.phone_number),
                                hour
                            ),
                        },
                        BotAction::SendButtons {
                            to: context.advisor_phone.clone(),
                            body: "Selecciona la respuesta para este caso.".to_string(),
                            buttons: vec![
                                reply_button(
                                    &advisor_button_id(
                                        ADVISOR_YES_HOUR_PREFIX,
                                        &context.phone_number,
                                    ),
                                    &advisor_title("Sí, confirmo", &context.phone_number),
                                ),
                                reply_button(
                                    &advisor_button_id(
                                        ADVISOR_OTHER_HOUR_PREFIX,
                                        &context.phone_number,
                                    ),
                                    &advisor_title("Otra hora", &context.phone_number),
                                ),
                            ],
                        },
                        BotAction::StartTimer {
                            timer_type: TimerType::AdvisorResponse,
                            phone: context.phone_number.clone(),
                            duration: ADVISOR_STUCK_TIMEOUT,
                        },
                    ],
                ))
            }
            Err(message) => Ok((
                ConversationState::WaitClientHour,
                vec![BotAction::SendText {
                    to: context.phone_number.clone(),
                    body: message,
                }],
            )),
        },
        _ => Ok((
            ConversationState::WaitClientHour,
            vec![BotAction::SendText {
                to: context.phone_number.clone(),
                body: client_messages()
                    .advisor_customer
                    .client_hour_retry_non_text
                    .clone(),
            }],
        )),
    }
}

pub(crate) fn start_waiting_for_contact_advisor(
    context: &mut ConversationContext,
) -> (ConversationState, Vec<BotAction>) {
    context.advisor_timer_started_at = Some(chrono::Utc::now());
    context.advisor_timer_expired = false;
    context.relay_kind = None;
    context.relay_timer_started_at = None;

    (
        ConversationState::WaitAdvisorContact,
        vec![
            BotAction::SendText {
                to: context.advisor_phone.clone(),
                body: render_contact_request(context),
            },
            BotAction::SendButtons {
                to: context.advisor_phone.clone(),
                body: "Selecciona cómo deseas responder.".to_string(),
                buttons: vec![reply_button(
                    &advisor_button_id(ADVISOR_ATTEND_PREFIX, &context.phone_number),
                    &advisor_title("Atender", &context.phone_number),
                )],
            },
            BotAction::SendText {
                to: context.phone_number.clone(),
                body: client_messages()
                    .advisor_customer
                    .wait_contact_initial_text
                    .clone(),
            },
            BotAction::StartTimer {
                timer_type: TimerType::AdvisorResponse,
                phone: context.phone_number.clone(),
                duration: ADVISOR_RESPONSE_TIMEOUT,
            },
        ],
    )
}

pub(crate) fn final_order_packet_actions(
    context: &ConversationContext,
    receipt_media_id: Option<&str>,
) -> Vec<BotAction> {
    let pedido = calcular_pedido(&context.items);
    let mut actions = vec![
        BotAction::SendText {
            to: context.advisor_phone.clone(),
            body: render_order_summary(context, &pedido),
        },
        BotAction::SendText {
            to: context.advisor_phone.clone(),
            body: render_final_order_status(context, receipt_media_id.is_some()),
        },
    ];

    if let Some(media_id) = receipt_media_id {
        actions.push(BotAction::SendImage {
            to: context.advisor_phone.clone(),
            media_id: media_id.to_string(),
            caption: Some(format!(
                "Comprobante {}",
                phone_marker(&context.phone_number)
            )),
        });
    }

    actions
}

fn ask_delivery_cost_entry_actions(
    context: &ConversationContext,
    pedido: &PedidoCalculado,
) -> Vec<BotAction> {
    vec![
        BotAction::FinalizeCurrentOrder {
            status: "pending_advisor".to_string(),
        },
        BotAction::BindAdvisorSession {
            advisor_phone: context.advisor_phone.clone(),
            target_phone: context.phone_number.clone(),
        },
        BotAction::SendText {
            to: context.advisor_phone.clone(),
            body: render_order_summary(context, pedido),
        },
        BotAction::SendText {
            to: context.advisor_phone.clone(),
            body: format!(
                "¿Cuánto cobra de domicilio para {}?",
                context
                    .delivery_address
                    .as_deref()
                    .unwrap_or("dirección pendiente")
            ),
        },
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: client_messages()
                .advisor_customer
                .wait_delivery_cost_text
                .clone(),
        },
        BotAction::StartTimer {
            timer_type: TimerType::AdvisorResponse,
            phone: context.phone_number.clone(),
            duration: ADVISOR_STUCK_TIMEOUT,
        },
    ]
}

fn wait_advisor_response_entry_actions(
    context: &ConversationContext,
    _pedido: &PedidoCalculado,
) -> Vec<BotAction> {
    vec![
        BotAction::SendText {
            to: context.advisor_phone.clone(),
            body: format!(
                "Perfecto. Ahora confirma disponibilidad para {}.",
                phone_marker(&context.phone_number)
            ),
        },
        BotAction::SendButtons {
            to: context.advisor_phone.clone(),
            body: "Selecciona cómo deseas responder a este pedido.".to_string(),
            buttons: advisor_order_buttons(context),
        },
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: client_messages()
                .advisor_customer
                .availability_wait_text
                .clone(),
        },
        BotAction::StartTimer {
            timer_type: TimerType::AdvisorResponse,
            phone: context.phone_number.clone(),
            duration: ADVISOR_AUTO_CANNOT_TIMEOUT,
        },
    ]
}

fn wait_advisor_mayor_entry_actions(
    context: &ConversationContext,
    pedido: &PedidoCalculado,
) -> Vec<BotAction> {
    let mut actions = vec![
        BotAction::FinalizeCurrentOrder {
            status: "pending_advisor".to_string(),
        },
        BotAction::SendText {
            to: context.advisor_phone.clone(),
            body: render_order_summary(context, pedido),
        },
    ];

    if let Some(receipt_media_id) = context.receipt_media_id.as_ref() {
        actions.push(BotAction::SendImage {
            to: context.advisor_phone.clone(),
            media_id: receipt_media_id.clone(),
            caption: Some(format!(
                "Comprobante {}",
                phone_marker(&context.phone_number)
            )),
        });
    }

    actions.extend([
        BotAction::SendButtons {
            to: context.advisor_phone.clone(),
            body: "Pedido al por mayor pendiente.".to_string(),
            buttons: vec![reply_button(
                &advisor_button_id(ADVISOR_TAKE_PREFIX, &context.phone_number),
                &advisor_title("Tomar pedido", &context.phone_number),
            )],
        },
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: client_messages()
                .advisor_customer
                .wholesale_order_sent_text
                .clone(),
        },
        BotAction::StartTimer {
            timer_type: TimerType::AdvisorResponse,
            phone: context.phone_number.clone(),
            duration: ADVISOR_RESPONSE_TIMEOUT,
        },
    ]);

    actions
}

fn relay_entry_actions(
    context: &ConversationContext,
    client_message: &str,
    advisor_message: &str,
    update_order_status: bool,
) -> Vec<BotAction> {
    let mut actions = vec![BotAction::CancelTimer {
        timer_type: TimerType::AdvisorResponse,
        phone: context.phone_number.clone(),
    }];

    if update_order_status {
        actions.push(BotAction::FinalizeCurrentOrder {
            status: "manual_followup".to_string(),
        });
    }

    actions.extend([
        BotAction::BindAdvisorSession {
            advisor_phone: context.advisor_phone.clone(),
            target_phone: context.phone_number.clone(),
        },
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: client_message.to_string(),
        },
        BotAction::SendText {
            to: context.advisor_phone.clone(),
            body: format!(
                "{} {}",
                advisor_message,
                phone_marker(&context.phone_number)
            ),
        },
        BotAction::StartTimer {
            timer_type: TimerType::RelayInactivity,
            phone: context.phone_number.clone(),
            duration: Duration::from_secs(30 * 60),
        },
        BotAction::SendButtons {
            to: context.advisor_phone.clone(),
            body: "Cuando cierres esta conversación, usa el botón para finalizar el relay."
                .to_string(),
            buttons: vec![reply_button(
                &advisor_button_id(ADVISOR_FINISH_PREFIX, &context.phone_number),
                &advisor_title("Finalizar", &context.phone_number),
            )],
        },
    ]);

    actions
}

fn transition_to_payment_selection(context: &mut ConversationContext) -> TransitionResult {
    context.advisor_timer_started_at = None;
    context.advisor_timer_expired = false;

    let mut actions = vec![
        BotAction::CancelTimer {
            timer_type: TimerType::AdvisorResponse,
            phone: context.phone_number.clone(),
        },
        BotAction::ClearAdvisorSession {
            advisor_phone: context.advisor_phone.clone(),
        },
        BotAction::UpsertDraftOrder {
            status: "draft_payment".to_string(),
        },
        BotAction::SendText {
            to: context.advisor_phone.clone(),
            body: format!(
                "Pedido {} listo para que el cliente elija el pago.",
                phone_marker(&context.phone_number)
            ),
        },
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: checkout::render_payment_ready_confirmation(context),
        },
    ];
    actions.extend(checkout::select_payment_method_actions(
        &context.phone_number,
    ));

    Ok((ConversationState::SelectPaymentMethod, actions))
}

fn render_order_summary(context: &ConversationContext, pedido: &PedidoCalculado) -> String {
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

    let pago = match context.payment_method.as_deref() {
        Some("cash_on_delivery") => "Contra entrega".to_string(),
        Some("transfer") => "Pago por transferencia".to_string(),
        Some(other) => other.to_string(),
        None => "Pendiente con cliente".to_string(),
    };

    let totals = match (context.delivery_cost, context.total_final) {
        (Some(delivery_cost), Some(total_final)) => format!(
            "\n\nSubtotal: {}\nDomicilio: {}\nTotal final: {}",
            format_currency(pedido.total_estimado),
            format_currency(u32::try_from(delivery_cost).unwrap_or_default()),
            format_currency(u32::try_from(total_final).unwrap_or_default()),
        ),
        _ => format!(
            "\n\nTotal estimado: {}",
            format_currency(pedido.total_estimado)
        ),
    };

    format!(
        "Pedido {}\n\nCliente: {}\nTeléfono: {}\nDirección: {}\nEntrega: {}\nPago: {}\n\nItems:\n{}{}",
        phone_marker(&context.phone_number),
        context.customer_name.as_deref().unwrap_or("pendiente"),
        context.customer_phone.as_deref().unwrap_or("pendiente"),
        context.delivery_address.as_deref().unwrap_or("pendiente"),
        entrega,
        pago,
        render_items(&pedido.items_detalle),
        totals,
    )
}

fn render_final_order_status(context: &ConversationContext, receipt_attached: bool) -> String {
    let pago = match context.payment_method.as_deref() {
        Some("cash_on_delivery") => "contra entrega",
        Some("transfer") => "pago ahora",
        Some(other) => other,
        None => "pendiente",
    };

    if receipt_attached {
        return format!(
            "Pedido {} confirmado. Pago registrado por transferencia; revisa el comprobante adjunto.",
            phone_marker(&context.phone_number)
        );
    }

    format!(
        "Pedido {} confirmado. Método de pago final: {}.",
        phone_marker(&context.phone_number),
        pago
    )
}

fn render_contact_request(context: &ConversationContext) -> String {
    format!(
        "Cliente quiere hablar con asesor {}\n\nNombre: {}\nTeléfono: {}",
        phone_marker(&context.phone_number),
        context.customer_name.as_deref().unwrap_or("pendiente"),
        context.customer_phone.as_deref().unwrap_or("pendiente"),
    )
}

fn render_left_message(context: &ConversationContext, message: &str) -> String {
    format!(
        "Mensaje para asesor {}\n\nNombre: {}\nTeléfono: {}\nMensaje: {}",
        phone_marker(&context.phone_number),
        context.customer_name.as_deref().unwrap_or("pendiente"),
        context.customer_phone.as_deref().unwrap_or("pendiente"),
        message,
    )
}

fn advisor_order_buttons(context: &ConversationContext) -> Vec<Button> {
    vec![reply_button(
        &advisor_button_id(ADVISOR_CONFIRM_PREFIX, &context.phone_number),
        &advisor_title("Confirmar", &context.phone_number),
    )]
}

fn transition_immediate_order_to_scheduled(context: &mut ConversationContext) -> TransitionResult {
    context.delivery_type = Some("scheduled".to_string());
    context.scheduled_date = Some(today_bogota_iso_date());
    context.scheduled_time = None;
    context.advisor_proposed_hour = None;
    context.client_counter_hour = None;
    context.advisor_timer_started_at = Some(chrono::Utc::now());
    context.advisor_timer_expired = false;

    Ok((
        ConversationState::NegotiateHour,
        vec![
            BotAction::CancelTimer {
                timer_type: TimerType::AdvisorResponse,
                phone: context.phone_number.clone(),
            },
            BotAction::BindAdvisorSession {
                advisor_phone: context.advisor_phone.clone(),
                target_phone: context.phone_number.clone(),
            },
            BotAction::SendText {
                to: context.advisor_phone.clone(),
                body: format!(
                    "Pedido {} quedó como programado para hoy. ¿Qué hora puede proponer?",
                    phone_marker(&context.phone_number)
                ),
            },
            BotAction::SendText {
                to: context.phone_number.clone(),
                body: client_messages()
                    .advisor_customer
                    .wait_negotiate_hour_text
                    .clone(),
            },
            BotAction::StartTimer {
                timer_type: TimerType::AdvisorResponse,
                phone: context.phone_number.clone(),
                duration: ADVISOR_STUCK_TIMEOUT,
            },
        ],
    ))
}

fn today_bogota_iso_date() -> String {
    let offset = FixedOffset::west_opt(5 * 3600).expect("valid Bogota offset");
    Utc::now()
        .with_timezone(&offset)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

fn render_items(items: &[ItemCalculated]) -> String {
    items
        .iter()
        .map(|item| {
            let tipo = if item.has_liquor {
                "con licor"
            } else {
                "sin licor"
            };
            let modo = if item.is_wholesale { "mayor" } else { "detal" };

            format!(
                "- {} x {} ({}, {}) -> {}",
                item.quantity,
                item.flavor,
                tipo,
                modo,
                format_currency(item.subtotal)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn advisor_button_action(input: &UserInput) -> Option<AdvisorButtonAction> {
    match input {
        UserInput::ButtonPress(id) | UserInput::ListSelection(id) => {
            parse_advisor_button_id(id).map(|(action, _)| action)
        }
        _ => None,
    }
}

fn parse_delivery_cost(input: &str) -> Result<i32, String> {
    let cost = input.trim().parse::<i32>().map_err(|_| {
        "Por favor envíe solo el valor numérico del domicilio (ej: 5000).".to_string()
    })?;

    if cost <= 0 {
        return Err("El domicilio debe ser un número entero positivo.".to_string());
    }

    Ok(cost)
}

fn validate_hour_text(input: &str) -> Result<String, String> {
    let normalized = collapse_spaces(input);
    let length = normalized.chars().count();

    if !(HOUR_MIN_LEN..=HOUR_MAX_LEN).contains(&length) {
        return Err("La hora debe tener entre 1 y 40 caracteres.".to_string());
    }

    Ok(normalized)
}

fn validate_client_hour_text(input: &str) -> Result<String, String> {
    let normalized = collapse_spaces(input);
    let length = normalized.chars().count();

    if !(HOUR_MIN_LEN..=HOUR_MAX_LEN).contains(&length) {
        return Err(client_messages().advisor_customer.hour_length_error.clone());
    }

    Ok(normalized)
}

fn validate_message(input: &str) -> Result<String, String> {
    let normalized = collapse_spaces(input);
    let length = normalized.chars().count();

    if !(LEAVE_MESSAGE_MIN_LEN..=LEAVE_MESSAGE_MAX_LEN).contains(&length) {
        return Err(client_messages()
            .advisor_customer
            .leave_message_length_error
            .clone());
    }

    Ok(normalized)
}

fn collapse_spaces(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn selection_id(input: &UserInput) -> Option<String> {
    match input {
        UserInput::ButtonPress(id) | UserInput::ListSelection(id) => Some(id.clone()),
        UserInput::TextMessage(text) if text.trim().eq_ignore_ascii_case("menu") => {
            Some(BACK_MAIN_MENU.to_string())
        }
        _ => None,
    }
}

fn advisor_button_id(prefix: &str, phone: &str) -> String {
    format!("{prefix}{phone}")
}

fn advisor_title(title: &str, phone: &str) -> String {
    format!("{title} {}", phone_button_suffix(phone))
}

pub(crate) fn phone_marker(phone: &str) -> String {
    let suffix = if phone.len() >= 4 {
        &phone[phone.len() - 4..]
    } else {
        phone
    };
    format!("[...{suffix}]")
}

fn phone_button_suffix(phone: &str) -> &str {
    if phone.len() >= 4 {
        &phone[phone.len() - 4..]
    } else {
        phone
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

fn retry_actions(phone: &str, message: &str, mut actions: Vec<BotAction>) -> Vec<BotAction> {
    let mut all = vec![BotAction::SendText {
        to: phone.to_string(),
        body: message.to_string(),
    }];
    all.append(&mut actions);
    all
}

fn reset_to_main_menu(
    context: &ConversationContext,
    cancel_order: bool,
) -> (ConversationState, Vec<BotAction>) {
    let mut actions = vec![
        BotAction::CancelTimer {
            timer_type: TimerType::AdvisorResponse,
            phone: context.phone_number.clone(),
        },
        BotAction::ClearAdvisorSession {
            advisor_phone: context.advisor_phone.clone(),
        },
    ];

    if cancel_order {
        if let Some(order_id) = context.current_order_id {
            actions.push(BotAction::CancelCurrentOrder { order_id });
        }
    }

    actions.push(BotAction::ResetConversation {
        phone: context.phone_number.clone(),
    });
    actions.extend(menu::main_menu_actions(&context.phone_number));

    (ConversationState::MainMenu, actions)
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

    use super::{
        handle_advisor_ask_delivery_cost, handle_advisor_confirm_hour,
        handle_advisor_wait_advisor_response, handle_client_waiting_state, handle_leave_message,
        parse_advisor_button_id, start_contact_advisor, AdvisorButtonAction,
    };

    fn context() -> ConversationContext {
        ConversationContext {
            phone_number: "573001234567".to_string(),
            advisor_phone: "573009999999".to_string(),
            customer_name: Some("Ana".to_string()),
            customer_phone: Some("3001234567".to_string()),
            delivery_address: Some("Cra 15 #20-30 Armenia".to_string()),
            items: vec![crate::db::models::OrderItemData {
                flavor: "Maracumango".to_string(),
                has_liquor: true,
                quantity: 2,
            }],
            delivery_type: Some("immediate".to_string()),
            scheduled_date: None,
            scheduled_time: None,
            customer_review_scope: None,
            payment_method: Some("cash_on_delivery".to_string()),
            delivery_cost: None,
            total_final: None,
            receipt_media_id: None,
            receipt_timer_started_at: None,
            advisor_target_phone: None,
            advisor_timer_started_at: Some(chrono::Utc::now()),
            advisor_timer_expired: false,
            relay_timer_started_at: None,
            relay_kind: None,
            advisor_proposed_hour: None,
            client_counter_hour: None,
            schedule_resume_target: None,
            current_order_id: Some(11),
            editing_address: false,
            receipt_timer_expired: false,
            pending_has_liquor: None,
            pending_flavor: None,
            conversation_abandon_started_at: None,
            conversation_abandon_reminder_sent: false,
        }
    }

    #[test]
    fn parses_button_target_phone() {
        let parsed = parse_advisor_button_id("advisor_confirm_573001234567").expect("button");

        assert_eq!(
            parsed,
            (AdvisorButtonAction::Confirm, "573001234567".to_string())
        );
    }

    #[test]
    fn advisor_confirm_for_immediate_order_moves_to_payment_selection() {
        let mut context = context();
        context.delivery_cost = Some(5000);
        context.total_final = Some(17000);

        let (state, actions) = handle_advisor_wait_advisor_response(
            &UserInput::ButtonPress("advisor_confirm_573001234567".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectPaymentMethod);
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::UpsertDraftOrder { status }
                if status == "draft_payment"
        )));
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::SendButtons { buttons, .. }
                if buttons.iter().any(|button| button.reply.id == "cash_on_delivery")
        )));
    }

    #[test]
    fn stale_cannot_button_is_ignored_in_wait_advisor_response() {
        let mut context = context();
        context.delivery_cost = Some(5000);
        context.total_final = Some(17000);

        let (state, actions) = handle_advisor_wait_advisor_response(
            &UserInput::ButtonPress("advisor_cannot_573001234567".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::WaitAdvisorResponse);
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::SendText { body, .. }
                if body.contains("Usa uno de los botones del caso")
        )));
    }

    #[test]
    fn advisor_confirm_for_scheduled_order_moves_to_payment_selection() {
        let mut context = context();
        context.delivery_type = Some("scheduled".to_string());
        context.scheduled_date = Some("2030-12-24".to_string());
        context.scheduled_time = Some("4:00 pm".to_string());
        context.delivery_cost = Some(5000);
        context.total_final = Some(17000);

        let (state, actions) = handle_advisor_confirm_hour(
            &UserInput::ButtonPress("advisor_confirm_573001234567".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectPaymentMethod);
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::UpsertDraftOrder { status }
                if status == "draft_payment"
        )));
    }

    #[test]
    fn advisor_delivery_cost_updates_total() {
        let mut context = context();

        let (state, actions) = handle_advisor_ask_delivery_cost(
            &UserInput::TextMessage("5000".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::WaitAdvisorResponse);
        assert!(actions.iter().any(|action| matches!(action, crate::bot::state_machine::BotAction::UpdateCurrentOrderDeliveryCost { delivery_cost, total_final, .. } if *delivery_cost == 5000 && *total_final == 17000)));
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::SendButtons { buttons, .. }
                if buttons.len() == 1 && buttons[0].reply.id == "advisor_confirm_573001234567"
        )));
        assert_eq!(context.delivery_cost, Some(5000));
        assert_eq!(context.total_final, Some(17000));
    }

    #[test]
    fn timed_out_wait_state_auto_moves_to_negotiation() {
        let mut context = context();
        context.advisor_timer_expired = true;

        let (state, _actions) = handle_client_waiting_state(
            &ConversationState::WaitAdvisorResponse,
            &UserInput::ButtonPress("advisor_timeout_schedule".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::NegotiateHour);
        assert_eq!(context.delivery_type.as_deref(), Some("scheduled"));
        assert_eq!(context.payment_method.as_deref(), Some("cash_on_delivery"));
    }

    #[test]
    fn scheduled_resume_preserves_wholesale_wait_state() {
        let mut context = context();
        context.schedule_resume_target = Some("wait_advisor_mayor".to_string());

        let (state, actions) = super::resume_after_schedule_confirmation(&mut context);

        assert_eq!(state, ConversationState::WaitAdvisorMayor);
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::SendButtons { buttons, .. }
                if buttons.iter().any(|button| button.reply.id == "advisor_take_573001234567")
        )));
    }

    #[test]
    fn start_contact_advisor_skips_data_if_already_present() {
        let mut context = context();

        let (state, actions) = start_contact_advisor(&mut context);

        assert_eq!(state, ConversationState::ConfirmCustomerData);
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::SendButtons { .. }
        )));
    }

    #[test]
    fn client_counter_hour_starts_stuck_advisor_timer() {
        let mut context = context();

        let (state, actions) = handle_client_waiting_state(
            &ConversationState::WaitClientHour,
            &UserInput::TextMessage("6:30 pm".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(
            state,
            ConversationState::WaitAdvisorHourDecision {
                client_hour: "6:30 pm".to_string()
            }
        );
        assert!(actions.iter().any(|action| matches!(
            action,
            crate::bot::state_machine::BotAction::StartTimer { timer_type: crate::bot::state_machine::TimerType::AdvisorResponse, duration, .. }
                if *duration == crate::bot::timers::ADVISOR_STUCK_TIMEOUT
        )));
    }

    #[test]
    fn leave_message_requires_text() {
        let mut context = context();

        let (state, _actions) =
            handle_leave_message(&UserInput::ImageMessage("x".to_string()), &mut context)
                .expect("transition");

        assert_eq!(state, ConversationState::LeaveMessage);
    }
}
