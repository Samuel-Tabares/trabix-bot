use std::time::Duration;

use chrono::Utc;

use crate::ai::agent::checkout_precondition_error;
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
    //
    // El mismo criterio se extiende (2026-08-30) a todo el tramo posterior a
    // que el pedido queda gestionado (ítems, datos del cliente y entrega ya
    // conocidos, `checkout_precondition_error` en None): a partir de ahí el
    // recordatorio de "¿sigues por ahí?" ya no aporta nada, solo interrumpe.
    let order_already_gestioned =
        context.order_confirmed || checkout_precondition_error(context).is_none();

    if transition_resets_conversation
        || order_just_confirmed
        || order_already_gestioned
        || !uses_customer_inactivity_timer(state)
    {
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
    use crate::bot::state_machine::{BotAction, ConversationState, TimerType};

    use super::{sync_customer_inactivity_timer, uses_customer_inactivity_timer, ConversationContext};

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

    fn empty_context() -> ConversationContext {
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
            referral_code: None,
            referral_has_boost: false,
            referral_discount_total: None,
            ambassador_commission_total: None,
            delivery_cost: None,
            total_final: None,
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
            order_confirmed: false,
            order_confirmed_at: None,
            confirmed_order_snapshot: None,
            referral_prompt_resolved: false,
            has_greeted: false,
            meta_customer_name: None,
            meta_customer_phone: None,
            pending_zone_kind: None,
            pending_zone_value: None,
            pending_zone_label: None,
        }
    }

    fn started_timer(actions: &[BotAction]) -> bool {
        actions
            .iter()
            .any(|a| matches!(a, BotAction::StartTimer { timer_type: TimerType::ConversationAbandon, .. }))
    }

    #[test]
    fn still_arms_timer_while_checkout_is_incomplete() {
        let mut context = empty_context();
        let actions =
            sync_customer_inactivity_timer(&ConversationState::MainMenu, &mut context, false, false);
        assert!(started_timer(&actions));
    }

    #[test]
    fn stops_arming_timer_once_checkout_preconditions_are_met() {
        let mut context = empty_context();
        context.items.push(crate::db::models::OrderItemData {
            flavor: "Mora".to_string(),
            has_liquor: true,
            quantity: 2,
        });
        context.customer_name = Some("Ana".to_string());
        context.customer_phone = Some("3001234567".to_string());
        context.delivery_address = Some("Cra 15 #20-30 Armenia".to_string());
        context.delivery_type = Some("immediate".to_string());

        let actions = sync_customer_inactivity_timer(
            &ConversationState::ReviewCheckout,
            &mut context,
            false,
            false,
        );
        assert!(!started_timer(&actions));
    }

    #[test]
    fn stops_arming_timer_once_order_is_confirmed() {
        let mut context = empty_context();
        context.order_confirmed = true;

        let actions =
            sync_customer_inactivity_timer(&ConversationState::MainMenu, &mut context, false, false);
        assert!(!started_timer(&actions));
    }
}
