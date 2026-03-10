use crate::bot::{
    state_machine::{
        BotAction, ConversationContext, ConversationState, TransitionResult, UserInput,
    },
    states::{data_collect, menu},
};

pub fn handle_contact_advisor_name(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match data_collect::validate_name(text) {
            Ok(name) => {
                context.customer_name = Some(name);
                Ok((
                    ConversationState::ContactAdvisorPhone,
                    contact_advisor_phone_actions(&context.phone_number),
                ))
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
                "Escribe tu nombre para continuar.",
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
                Ok((
                    ConversationState::WaitAdvisorContact,
                    wait_advisor_contact_actions(&context.phone_number),
                ))
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
                "Escribe un teléfono válido para continuar.",
                contact_advisor_phone_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn handle_wait_advisor_contact(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    if wants_menu(input) {
        return Ok((
            ConversationState::MainMenu,
            menu::main_menu_actions(&context.phone_number),
        ));
    }

    Ok((
        ConversationState::WaitAdvisorContact,
        wait_advisor_contact_actions(&context.phone_number),
    ))
}

pub fn handle_leave_message(
    _input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    Ok((
        ConversationState::LeaveMessage,
        vec![BotAction::SendText {
            to: context.phone_number.clone(),
            body: "La gestión de mensajes al asesor se implementa en Fase 4.".to_string(),
        }],
    ))
}

pub fn handle_unimplemented(
    state: &ConversationState,
    context: &mut ConversationContext,
) -> TransitionResult {
    Ok((
        state.clone(),
        vec![BotAction::SendText {
            to: context.phone_number.clone(),
            body: "Esta parte del flujo se implementa en una fase posterior.".to_string(),
        }],
    ))
}

pub fn contact_advisor_name_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: "¿Nombre del cliente?".to_string(),
    }]
}

pub fn contact_advisor_phone_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: "¿Teléfono de contacto?".to_string(),
    }]
}

pub fn wait_advisor_contact_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: "La atención con asesor real se implementa en Fase 4. Por ahora puedes escribir 'menu' para volver al menú principal.".to_string(),
    }]
}

fn retry_actions(phone: &str, message: &str, mut actions: Vec<BotAction>) -> Vec<BotAction> {
    let mut all = vec![BotAction::SendText {
        to: phone.to_string(),
        body: message.to_string(),
    }];
    all.append(&mut actions);
    all
}

fn wants_menu(input: &UserInput) -> bool {
    match input {
        UserInput::TextMessage(text) => text.trim().eq_ignore_ascii_case("menu"),
        _ => false,
    }
}

