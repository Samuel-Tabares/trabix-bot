use crate::bot::state_machine::{
    BotAction, ConversationContext, ConversationState, TransitionResult, UserInput,
};

pub fn handle_relay_mode(
    _input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    Ok((
        ConversationState::RelayMode,
        vec![BotAction::SendText {
            to: context.phone_number.clone(),
            body: "El modo relay se implementa en Fase 4.".to_string(),
        }],
    ))
}
