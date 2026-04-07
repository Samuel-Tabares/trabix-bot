use std::time::Duration;

use chrono::Utc;

use crate::{
    bot::{
        state_machine::{BotAction, ConversationContext, ConversationState, TimerType},
        states::{advisor, checkout, customer_data, data_collect, menu, order, scheduling},
    },
    messages::client_messages,
};

pub const CONVERSATION_REMINDER_TIMEOUT: Duration = Duration::from_secs(2 * 60);
pub const CONVERSATION_RESET_TIMEOUT: Duration = Duration::from_secs(35 * 60);

pub fn sync_customer_inactivity_timer(
    state: &ConversationState,
    context: &mut ConversationContext,
    transition_resets_conversation: bool,
) -> Vec<BotAction> {
    let phone = context.phone_number.clone();

    if transition_resets_conversation || !uses_customer_inactivity_timer(state) {
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
            | ConversationState::SelectPaymentMethod
            | ConversationState::OfferHourToClient { .. }
            | ConversationState::WaitClientHour
            | ConversationState::ContactAdvisorName
            | ConversationState::ContactAdvisorPhone
            | ConversationState::LeaveMessage
    )
}

pub fn reminder_actions(
    state: &ConversationState,
    context: &ConversationContext,
) -> Vec<BotAction> {
    match state {
        ConversationState::MainMenu => menu::main_menu_actions(&context.phone_number),
        ConversationState::ViewMenu => menu::view_menu_actions(&context.phone_number),
        ConversationState::ViewSchedule => menu::main_menu_actions(&context.phone_number),
        ConversationState::WhenDelivery => scheduling::when_delivery_actions(&context.phone_number),
        ConversationState::OutOfHours => scheduling::out_of_hours_actions(&context.phone_number),
        ConversationState::SelectDate => scheduling::select_date_actions(&context.phone_number),
        ConversationState::SelectTime => scheduling::select_time_actions(&context.phone_number),
        ConversationState::ConfirmSchedule => scheduling::confirm_schedule_actions(context),
        ConversationState::CollectName => data_collect::collect_name_actions(&context.phone_number),
        ConversationState::CollectPhone => {
            data_collect::collect_phone_actions(&context.phone_number)
        }
        ConversationState::CollectAddress => {
            data_collect::collect_address_actions(&context.phone_number)
        }
        ConversationState::SelectType => order::select_type_actions(&context.phone_number),
        ConversationState::SelectFlavor { has_liquor } => {
            order::select_flavor_actions(&context.phone_number, *has_liquor)
        }
        ConversationState::SelectQuantity { has_liquor, flavor } => {
            order::select_quantity_actions(&context.phone_number, *has_liquor, flavor)
        }
        ConversationState::AddMore => order::add_more_actions(context),
        ConversationState::ConfirmRestartOrder => {
            order::confirm_restart_order_actions(&context.phone_number)
        }
        ConversationState::ConfirmCustomerData => checkout::confirm_address_actions(context),
        ConversationState::SelectCustomerDataField => {
            customer_data::select_customer_data_field_actions(context)
        }
        ConversationState::EditCustomerName => customer_data::edit_customer_name_actions(context),
        ConversationState::EditCustomerPhone => customer_data::edit_customer_phone_actions(context),
        ConversationState::EditCustomerAddress => {
            checkout::change_address_prompt_actions(&context.phone_number)
        }
        ConversationState::ReviewCheckout => checkout::review_checkout_actions(context),
        ConversationState::SelectPaymentMethod => {
            checkout::select_payment_method_actions(&context.phone_number)
        }
        ConversationState::OfferHourToClient { proposed_hour } => {
            advisor::offer_hour_to_client_actions(&context.phone_number, proposed_hour)
        }
        ConversationState::WaitClientHour => {
            advisor::wait_client_hour_actions(&context.phone_number)
        }
        ConversationState::ContactAdvisorName => {
            advisor::contact_advisor_name_actions(&context.phone_number)
        }
        ConversationState::ContactAdvisorPhone => {
            advisor::contact_advisor_phone_actions(&context.phone_number)
        }
        ConversationState::LeaveMessage => {
            advisor::leave_message_prompt_actions(&context.phone_number)
        }
        _ => Vec::new(),
    }
}

pub fn reset_notice_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages()
            .timers_customer
            .conversation_inactivity_reset_text
            .clone(),
    }]
}

#[cfg(test)]
mod tests {
    use crate::bot::state_machine::ConversationState;

    use super::{reminder_actions, uses_customer_inactivity_timer};

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

    #[test]
    fn reminder_actions_for_legacy_view_schedule_use_main_menu() {
        let context = crate::bot::state_machine::ConversationContext {
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
        };

        let actions = reminder_actions(&ConversationState::ViewSchedule, &context);

        assert_eq!(actions.len(), 2);
        assert!(matches!(
            actions[1],
            crate::bot::state_machine::BotAction::SendButtons { .. }
        ));
    }
}
