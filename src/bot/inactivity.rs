use std::time::Duration;

use chrono::Utc;

use crate::bot::state_machine::{BotAction, ConversationContext, ConversationState, TimerType};

pub const CONVERSATION_REMINDER_TIMEOUT: Duration = Duration::from_secs(2 * 60);

pub fn sync_customer_inactivity_timer(
    state: &ConversationState,
    context: &mut ConversationContext,
    transition_resets_conversation: bool,
    order_just_confirmed: bool,
) -> Vec<BotAction> {
    let phone = context.phone_number.clone();

    // Un pedido recién confirmado aterriza en MainMenu, que sí usa este
    // timer — pero no hay nada pendiente que recordarle al cliente.
    // Encontrado en vivo (2026-08-12): a los 2 minutos de recibir "tu pedido
    // quedó confirmado" le llegaba "¿Sigues por ahí? cuando quieras seguimos
    // con tu pedido", contradiciendo lo que se le acababa de decir.
    if transition_resets_conversation || order_just_confirmed || !uses_customer_inactivity_timer(state) {
        clear_customer_inactivity_tracking(context);
        return vec![BotAction::CancelTimer {
            timer_type: TimerType::ConversationAbandon,
            phone,
        }];
    }

    context.conversation_abandon_started_at = Some(Utc::now());
    context.conversation_abandon_reminder_sent = false;

    vec![BotAction::StartTimer {
        timer_type: TimerType::ConversationAbandon,
        phone,
        duration: CONVERSATION_REMINDER_TIMEOUT,
    }]
}

pub fn clear_customer_inactivity_tracking(context: &mut ConversationContext) {
    context.conversation_abandon_started_at = None;
    context.conversation_abandon_reminder_sent = false;
}

pub fn uses_customer_inactivity_timer(state: &ConversationState) -> bool {
    matches!(
        state,
        ConversationState::MainMenu
            | ConversationState::ViewMenu
            | ConversationState::ViewSchedule
            | ConversationState::WhenDelivery
            | ConversationState::OutOfHours
            | ConversationState::SelectDate
            | ConversationState::SelectTime
            | ConversationState::ConfirmSchedule
            | ConversationState::CollectName
            | ConversationState::CollectPhone
            | ConversationState::CollectAddress
            | ConversationState::SelectType
            | ConversationState::SelectFlavor { .. }
            | ConversationState::SelectQuantity { .. }
            | ConversationState::AddMore
            | ConversationState::ConfirmRestartOrder
            | ConversationState::ConfirmCustomerData
            | ConversationState::SelectCustomerDataField
            | ConversationState::EditCustomerName
            | ConversationState::EditCustomerPhone
            | ConversationState::EditCustomerAddress
            | ConversationState::ReviewCheckout
            | ConversationState::SelectReferralOption
            | ConversationState::WaitReferralCode
            | ConversationState::SelectPaymentMethod
            | ConversationState::OfferHourToClient { .. }
            | ConversationState::WaitClientHour
            | ConversationState::ContactAdvisorName
            | ConversationState::ContactAdvisorPhone
            | ConversationState::LeaveMessage
    )
}

#[cfg(test)]
mod tests {
    use crate::bot::state_machine::ConversationState;

    use super::uses_customer_inactivity_timer;

    #[test]
    fn excludes_relay_and_existing_timed_states() {
        assert!(!uses_customer_inactivity_timer(
            &ConversationState::WaitReceipt
        ));
        assert!(!uses_customer_inactivity_timer(
            &ConversationState::WaitAdvisorResponse
        ));
        assert!(!uses_customer_inactivity_timer(
            &ConversationState::RelayMode
        ));
    }

    #[test]
    fn includes_main_menu() {
        assert!(uses_customer_inactivity_timer(&ConversationState::MainMenu));
    }
}
