use crate::{
    bot::{
        state_machine::{
            BotAction, ConversationContext, ConversationState, TimerType, TransitionResult,
            UserInput,
        },
        states::customer_data,
    },
    db::models::OrderItemData,
    messages::{client_messages, render_template},
    whatsapp::types::{Button, ButtonReplyPayload, ListRow, ListSection},
};

const WITH_LIQUOR: &str = "with_liquor";
const WITHOUT_LIQUOR: &str = "without_liquor";
const ADD_MORE: &str = "add_more";
const FINISH_ORDER: &str = "finish_order";
const RESTART_ORDER: &str = "restart_order";
const CONFIRM_RESTART_ORDER: &str = "confirm_restart_order";
const CANCEL_RESTART_ORDER: &str = "cancel_restart_order";

const LIQUOR_FLAVOR_IDS: [&str; 7] = [
    "liquor_maracumango_ron_blanco",
    "liquor_blueberry_vodka",
    "liquor_uva_vodka",
    "liquor_bonbonbum_whiskey",
    "liquor_bonbonbum_fresa_champagne",
    "liquor_smirnoff_lulo",
    "liquor_manzana_verde_tequila",
];

const NON_LIQUOR_FLAVOR_IDS: [&str; 4] = [
    "non_liquor_maracumango",
    "non_liquor_manzana_verde",
    "non_liquor_bonbonbum",
    "non_liquor_blueberry",
];

pub fn handle_select_type(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(WITH_LIQUOR) => {
            context.pending_has_liquor = Some(true);
            context.pending_flavor = None;
            Ok((
                ConversationState::SelectFlavor { has_liquor: true },
                select_flavor_actions(&context.phone_number, true),
            ))
        }
        Some(WITHOUT_LIQUOR) => {
            context.pending_has_liquor = Some(false);
            context.pending_flavor = None;
            Ok((
                ConversationState::SelectFlavor { has_liquor: false },
                select_flavor_actions(&context.phone_number, false),
            ))
        }
        _ => Ok((
            ConversationState::SelectType,
            retry_actions(
                &context.phone_number,
                &client_messages().order.retry_select_type,
                select_type_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn handle_select_flavor(
    input: &UserInput,
    context: &mut ConversationContext,
    has_liquor: bool,
) -> TransitionResult {
    match selection_id(input)
        .as_deref()
        .and_then(|id| flavor_by_id(id, has_liquor))
    {
        Some(flavor) => {
            context.pending_has_liquor = Some(has_liquor);
            context.pending_flavor = Some(flavor.clone());

            Ok((
                ConversationState::SelectQuantity {
                    has_liquor,
                    flavor: flavor.clone(),
                },
                select_quantity_actions(&context.phone_number, has_liquor, &flavor),
            ))
        }
        None => Ok((
            ConversationState::SelectFlavor { has_liquor },
            retry_actions(
                &context.phone_number,
                &client_messages().order.retry_select_flavor,
                select_flavor_actions(&context.phone_number, has_liquor),
            ),
        )),
    }
}

pub fn handle_select_quantity(
    input: &UserInput,
    context: &mut ConversationContext,
    has_liquor: bool,
    flavor: &str,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match validate_quantity(text) {
            Ok(quantity) => {
                context.items.push(OrderItemData {
                    flavor: flavor.to_string(),
                    has_liquor,
                    quantity,
                });
                context.clear_pending_selection();

                Ok((ConversationState::AddMore, add_more_actions(context)))
            }
            Err(message) => Ok((
                ConversationState::SelectQuantity {
                    has_liquor,
                    flavor: flavor.to_string(),
                },
                retry_actions(
                    &context.phone_number,
                    &message,
                    select_quantity_actions(&context.phone_number, has_liquor, flavor),
                ),
            )),
        },
        _ => Ok((
            ConversationState::SelectQuantity {
                has_liquor,
                flavor: flavor.to_string(),
            },
            retry_actions(
                &context.phone_number,
                &client_messages().order.retry_quantity_non_text,
                select_quantity_actions(&context.phone_number, has_liquor, flavor),
            ),
        )),
    }
}

pub fn handle_add_more(input: &UserInput, context: &mut ConversationContext) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(ADD_MORE) => Ok((
            ConversationState::SelectType,
            select_type_actions(&context.phone_number),
        )),
        Some(RESTART_ORDER) => Ok((
            ConversationState::ConfirmRestartOrder,
            confirm_restart_order_actions(&context.phone_number),
        )),
        Some(FINISH_ORDER) => Ok((
            ConversationState::ReviewCheckout,
            customer_data::start_checkout_review(context).1,
        )),
        _ => Ok((
            ConversationState::AddMore,
            retry_actions(
                &context.phone_number,
                &client_messages().order.retry_add_more,
                add_more_actions(context),
            ),
        )),
    }
}

pub fn handle_confirm_restart_order(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(CONFIRM_RESTART_ORDER) => Ok(restart_order_transition(context)),
        Some(CANCEL_RESTART_ORDER) => Ok((ConversationState::AddMore, add_more_actions(context))),
        _ => Ok((
            ConversationState::ConfirmRestartOrder,
            retry_actions(
                &context.phone_number,
                &client_messages().order.retry_confirm_restart_order,
                confirm_restart_order_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn select_type_actions(phone: &str) -> Vec<BotAction> {
    let messages = &client_messages().order;
    vec![BotAction::SendButtons {
        to: phone.to_string(),
        body: messages.select_type_body.clone(),
        buttons: vec![
            reply_button(WITH_LIQUOR, &messages.with_liquor_button),
            reply_button(WITHOUT_LIQUOR, &messages.without_liquor_button),
        ],
    }]
}

pub fn select_flavor_actions(phone: &str, has_liquor: bool) -> Vec<BotAction> {
    let messages = &client_messages().order;
    let (body, rows) = if has_liquor {
        (
            messages.select_flavor_with_liquor_body.clone(),
            flavor_rows(
                &LIQUOR_FLAVOR_IDS,
                &messages.flavors_with_liquor,
                &messages.flavor_row_description,
            ),
        )
    } else {
        (
            messages.select_flavor_without_liquor_body.clone(),
            flavor_rows(
                &NON_LIQUOR_FLAVOR_IDS,
                &messages.flavors_without_liquor,
                &messages.flavor_row_description,
            ),
        )
    };

    vec![BotAction::SendList {
        to: phone.to_string(),
        body,
        button_text: messages.flavor_button_text.clone(),
        sections: vec![ListSection {
            title: messages.flavor_section_title.clone(),
            rows,
        }],
    }]
}

pub fn select_quantity_actions(phone: &str, has_liquor: bool, flavor: &str) -> Vec<BotAction> {
    let messages = &client_messages().order;
    let kind = if has_liquor {
        messages.quantity_kind_with_liquor.as_str()
    } else {
        messages.quantity_kind_without_liquor.as_str()
    };
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: render_template(
            &messages.quantity_prompt_template,
            &[("flavor", flavor), ("kind", kind)],
        ),
    }]
}

pub fn add_more_actions(context: &ConversationContext) -> Vec<BotAction> {
    let messages = &client_messages().order;
    vec![
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: render_template(
                &messages.added_to_order_template,
                &[("summary", &partial_summary(&context.items))],
            ),
        },
        BotAction::SendButtons {
            to: context.phone_number.clone(),
            body: messages.add_more_body.clone(),
            buttons: vec![
                reply_button(ADD_MORE, &messages.add_more_button),
                reply_button(FINISH_ORDER, &messages.finish_order_button),
                reply_button(RESTART_ORDER, &messages.restart_order_button),
            ],
        },
    ]
}

pub fn confirm_restart_order_actions(phone: &str) -> Vec<BotAction> {
    let messages = &client_messages().order;
    vec![BotAction::SendButtons {
        to: phone.to_string(),
        body: messages.confirm_restart_order_body.clone(),
        buttons: vec![
            reply_button(
                CONFIRM_RESTART_ORDER,
                &messages.confirm_restart_order_button,
            ),
            reply_button(CANCEL_RESTART_ORDER, &messages.cancel_restart_order_button),
        ],
    }]
}

pub fn validate_quantity(input: &str) -> Result<u32, String> {
    let quantity = input
        .trim()
        .parse::<u32>()
        .map_err(|_| client_messages().order.quantity_parse_error.clone())?;

    if !(1..=999).contains(&quantity) {
        return Err(client_messages().order.quantity_range_error.clone());
    }

    Ok(quantity)
}

fn flavor_rows(
    flavor_ids: &[&str],
    titles: &std::collections::BTreeMap<String, String>,
    description: &str,
) -> Vec<ListRow> {
    flavor_ids
        .iter()
        .map(|id| ListRow {
            id: (*id).to_string(),
            title: titles
                .get(*id)
                .cloned()
                .unwrap_or_else(|| (*id).to_string()),
            description: description.to_string(),
        })
        .collect()
}

fn flavor_by_id(id: &str, has_liquor: bool) -> Option<String> {
    let messages = &client_messages().order;
    if has_liquor {
        messages.flavors_with_liquor.get(id)
    } else {
        messages.flavors_without_liquor.get(id)
    }
    .cloned()
}

fn partial_summary(items: &[OrderItemData]) -> String {
    let messages = &client_messages().order;

    items
        .iter()
        .map(|item| {
            let kind = if item.has_liquor {
                messages.partial_kind_with_liquor.as_str()
            } else {
                messages.partial_kind_without_liquor.as_str()
            };

            render_template(
                &messages.partial_summary_line_template,
                &[
                    ("quantity", &item.quantity.to_string()),
                    ("flavor", &item.flavor),
                    ("kind", kind),
                ],
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
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

fn restart_order_transition(
    context: &mut ConversationContext,
) -> (ConversationState, Vec<BotAction>) {
    let cancel_action = context
        .current_order_id
        .map(|order_id| BotAction::CancelCurrentOrder { order_id });

    context.items.clear();
    context.customer_review_scope = None;
    context.payment_method = None;
    context.delivery_cost = None;
    context.total_final = None;
    context.receipt_media_id = None;
    context.receipt_timer_started_at = None;
    context.current_order_id = None;
    context.editing_address = false;
    context.receipt_timer_expired = false;
    context.clear_pending_selection();

    let mut actions = vec![BotAction::CancelTimer {
        timer_type: TimerType::ReceiptUpload,
        phone: context.phone_number.clone(),
    }];
    if let Some(action) = cancel_action {
        actions.push(action);
    }
    actions.push(BotAction::SendText {
        to: context.phone_number.clone(),
        body: client_messages().order.restart_order_success_text.clone(),
    });
    actions.extend(select_type_actions(&context.phone_number));

    (ConversationState::SelectType, actions)
}

fn selection_id(input: &UserInput) -> Option<String> {
    match input {
        UserInput::ButtonPress(id) | UserInput::ListSelection(id) => Some(id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::bot::state_machine::{ConversationContext, ConversationState, UserInput};

    use super::{
        add_more_actions, confirm_restart_order_actions, handle_add_more,
        handle_confirm_restart_order, handle_select_flavor, handle_select_quantity,
        handle_select_type, select_flavor_actions, validate_quantity,
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
        }
    }

    #[test]
    fn validates_quantity() {
        assert_eq!(validate_quantity("12").unwrap(), 12);
    }

    #[test]
    fn select_type_sets_liquor_flag() {
        let mut context = context();
        let (state, _) = handle_select_type(
            &UserInput::ButtonPress("with_liquor".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectFlavor { has_liquor: true });
        assert_eq!(context.pending_has_liquor, Some(true));
    }

    #[test]
    fn select_flavor_actions_include_only_list() {
        let actions = select_flavor_actions("573001234567", true);

        assert!(matches!(
            actions.first(),
            Some(crate::bot::state_machine::BotAction::SendList { sections, .. })
            if sections.first().map(|section| section.rows.len()) == Some(7)
        ));
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn flavor_titles_fit_whatsapp_list_limit() {
        let messages = crate::messages::client_messages();

        for title in messages
            .order
            .flavors_with_liquor
            .values()
            .chain(messages.order.flavors_without_liquor.values())
        {
            assert!(
                title.chars().count() <= 24,
                "flavor title exceeds Meta list limit: {title}"
            );
        }
    }

    #[test]
    fn select_flavor_moves_to_quantity_from_list_selection() {
        let mut context = context();
        let (state, _) = handle_select_flavor(
            &UserInput::ListSelection("non_liquor_bonbonbum".to_string()),
            &mut context,
            false,
        )
        .expect("transition");

        assert_eq!(
            state,
            ConversationState::SelectQuantity {
                has_liquor: false,
                flavor: "Bonbonbum".to_string()
            }
        );
        assert_eq!(context.pending_flavor.as_deref(), Some("Bonbonbum"));
    }

    #[test]
    fn select_quantity_adds_item() {
        let mut context = context();
        let (state, _) = handle_select_quantity(
            &UserInput::TextMessage("3".to_string()),
            &mut context,
            true,
            "Maracumango Ron blanco",
        )
        .expect("transition");

        assert_eq!(state, ConversationState::AddMore);
        assert_eq!(context.items.len(), 1);
        assert_eq!(context.items[0].quantity, 3);
    }

    #[test]
    fn add_more_finish_moves_to_review() {
        let mut context = context();
        context.items.push(crate::db::models::OrderItemData {
            flavor: "Bonbonbum".to_string(),
            has_liquor: false,
            quantity: 2,
        });

        let (state, _) = handle_add_more(
            &UserInput::ButtonPress("finish_order".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::ReviewCheckout);
    }

    #[test]
    fn add_more_actions_include_restart_button() {
        let mut context = context();
        context.items.push(crate::db::models::OrderItemData {
            flavor: "Bonbonbum".to_string(),
            has_liquor: false,
            quantity: 2,
        });

        let actions = add_more_actions(&context);

        assert!(matches!(
            actions.get(1),
            Some(crate::bot::state_machine::BotAction::SendButtons { buttons, .. })
            if buttons.len() == 3
                && buttons.iter().any(|button| button.reply.id == "restart_order")
        ));
    }

    #[test]
    fn add_more_restart_moves_to_confirmation() {
        let mut context = context();
        context.items.push(crate::db::models::OrderItemData {
            flavor: "Bonbonbum".to_string(),
            has_liquor: false,
            quantity: 2,
        });

        let (state, actions) = handle_add_more(
            &UserInput::ButtonPress("restart_order".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::ConfirmRestartOrder);
        assert!(matches!(
            actions.first(),
            Some(crate::bot::state_machine::BotAction::SendButtons { buttons, .. })
            if buttons.len() == 2
        ));
    }

    #[test]
    fn confirm_restart_clears_items_and_preserves_delivery_context() {
        let mut context = context();
        context.delivery_type = Some("scheduled".to_string());
        context.scheduled_date = Some("2026-03-28".to_string());
        context.scheduled_time = Some("18:00".to_string());
        context.items.push(crate::db::models::OrderItemData {
            flavor: "Bonbonbum".to_string(),
            has_liquor: false,
            quantity: 2,
        });

        let (state, _) = handle_confirm_restart_order(
            &UserInput::ButtonPress("confirm_restart_order".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectType);
        assert!(context.items.is_empty());
        assert_eq!(context.delivery_type.as_deref(), Some("scheduled"));
        assert_eq!(context.scheduled_date.as_deref(), Some("2026-03-28"));
        assert_eq!(context.scheduled_time.as_deref(), Some("18:00"));
    }

    #[test]
    fn cancel_restart_returns_to_add_more_without_clearing_items() {
        let mut context = context();
        context.items.push(crate::db::models::OrderItemData {
            flavor: "Bonbonbum".to_string(),
            has_liquor: false,
            quantity: 2,
        });

        let (state, actions) = handle_confirm_restart_order(
            &UserInput::ButtonPress("cancel_restart_order".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::AddMore);
        assert_eq!(context.items.len(), 1);
        assert!(matches!(
            actions.get(1),
            Some(crate::bot::state_machine::BotAction::SendButtons { buttons, .. })
            if buttons.len() == 3
        ));
    }

    #[test]
    fn confirm_restart_actions_include_warning_buttons() {
        let actions = confirm_restart_order_actions("573001234567");

        assert!(matches!(
            actions.first(),
            Some(crate::bot::state_machine::BotAction::SendButtons { buttons, .. })
            if buttons.len() == 2
                && buttons.iter().any(|button| button.reply.id == "confirm_restart_order")
                && buttons.iter().any(|button| button.reply.id == "cancel_restart_order")
        ));
    }
}
