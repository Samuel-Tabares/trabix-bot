use crate::{
    bot::state_machine::{BotAction, ConversationContext, ConversationState},
    messages::{client_messages, render_template},
    whatsapp::types::{Button, ButtonReplyPayload},
};

use super::{advisor, checkout, data_collect, order};

pub const REVIEW_SCOPE_CHECKOUT: &str = "checkout_review";
pub const REVIEW_SCOPE_ADVISOR: &str = "advisor_contact";

const CONTINUE_CUSTOMER_DATA: &str = "continue_customer_data";
const CHANGE_CUSTOMER_DATA: &str = "change_customer_data";
const EDIT_CUSTOMER_NAME: &str = "edit_customer_name";
const EDIT_CUSTOMER_PHONE: &str = "edit_customer_phone";
const EDIT_CUSTOMER_ADDRESS: &str = "edit_customer_address";

pub fn next_order_data_state(
    context: &mut ConversationContext,
) -> (ConversationState, Vec<BotAction>) {
    context.customer_review_scope = None;

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

    (
        ConversationState::SelectType,
        order::select_type_actions(&context.phone_number),
    )
}

pub fn start_checkout_review(
    context: &mut ConversationContext,
) -> (ConversationState, Vec<BotAction>) {
    context.customer_review_scope = Some(REVIEW_SCOPE_CHECKOUT.to_string());
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

pub fn confirm_customer_data_actions(context: &ConversationContext) -> Vec<BotAction> {
    if review_scope(context) != REVIEW_SCOPE_ADVISOR {
        return checkout::review_checkout_actions(context);
    }

    let messages = &client_messages().advisor_customer;
    let body = render_template(
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
    );

    vec![BotAction::SendButtons {
        to: context.phone_number.clone(),
        body,
        buttons: vec![
            reply_button(
                CONTINUE_CUSTOMER_DATA,
                &messages.confirm_contact_continue_button,
            ),
            reply_button(
                CHANGE_CUSTOMER_DATA,
                &messages.confirm_contact_change_button,
            ),
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

pub fn edit_customer_address_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages().checkout.change_address_prompt.clone(),
    }]
}

fn enter_review_state(context: &mut ConversationContext) -> (ConversationState, Vec<BotAction>) {
    context.editing_address = false;
    if review_scope(context) == REVIEW_SCOPE_ADVISOR {
        (
            ConversationState::ConfirmCustomerData,
            confirm_customer_data_actions(context),
        )
    } else {
        (
            ConversationState::ReviewCheckout,
            checkout::review_checkout_actions(context),
        )
    }
}

fn review_scope(context: &ConversationContext) -> &str {
    context
        .customer_review_scope
        .as_deref()
        .unwrap_or(REVIEW_SCOPE_CHECKOUT)
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
    use crate::bot::state_machine::{ConversationContext, ConversationState};

    use super::{next_contact_advisor_state, next_order_data_state, REVIEW_SCOPE_ADVISOR};

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

    #[test]
    fn order_data_flow_skips_to_review_when_all_fields_exist() {
        let mut context = context();

        let (state, _) = next_order_data_state(&mut context);

        assert_eq!(state, ConversationState::SelectType);
        assert_eq!(context.customer_review_scope, None);
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

}
