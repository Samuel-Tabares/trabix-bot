use crate::{
    bot::{
        state_machine::{
            BotAction, ConversationContext, ConversationState, TransitionResult, UserInput,
        },
        states::checkout,
    },
    db::models::OrderItemData,
    whatsapp::types::{Button, ButtonReplyPayload},
};

const WITH_LIQUOR: &str = "with_liquor";
const WITHOUT_LIQUOR: &str = "without_liquor";
const ADD_MORE: &str = "add_more";
const FINISH_ORDER: &str = "finish_order";

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
                "Selecciona el tipo de granizado.",
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
    match input {
        UserInput::TextMessage(text) => {
            let flavor = normalize_flavor(text);
            if flavor.is_empty() {
                return Ok((
                    ConversationState::SelectFlavor { has_liquor },
                    retry_actions(
                        &context.phone_number,
                        "Escribe un sabor para continuar.",
                        select_flavor_actions(&context.phone_number, has_liquor),
                    ),
                ));
            }

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
        _ => Ok((
            ConversationState::SelectFlavor { has_liquor },
            retry_actions(
                &context.phone_number,
                "Escribe el sabor que deseas.",
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
                "Escribe una cantidad válida para continuar.",
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
        Some(FINISH_ORDER) => Ok((
            ConversationState::ShowSummary,
            checkout::show_summary_actions(context),
        )),
        _ => Ok((
            ConversationState::AddMore,
            retry_actions(
                &context.phone_number,
                "Selecciona si deseas agregar más o finalizar el pedido.",
                add_more_actions(context),
            ),
        )),
    }
}

pub fn select_type_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendButtons {
        to: phone.to_string(),
        body: "¿Qué tipo de granizado deseas?".to_string(),
        buttons: vec![
            reply_button(WITH_LIQUOR, "Con Licor"),
            reply_button(WITHOUT_LIQUOR, "Sin Licor"),
        ],
    }]
}

pub fn select_flavor_actions(phone: &str, has_liquor: bool) -> Vec<BotAction> {
    let body = if has_liquor {
        "Escribe el sabor con licor que deseas. Referencia provisional: maracuya, mora, fresa, mango."
    } else {
        "Escribe el sabor sin licor que deseas. Referencia provisional: mora, maracuya, limon, mango."
    };

    vec![BotAction::SendText {
        to: phone.to_string(),
        body: body.to_string(),
    }]
}

pub fn select_quantity_actions(phone: &str, has_liquor: bool, flavor: &str) -> Vec<BotAction> {
    let kind = if has_liquor { "con licor" } else { "sin licor" };
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: format!("¿Cuántos de {} {} deseas?", flavor, kind),
    }]
}

pub fn add_more_actions(context: &ConversationContext) -> Vec<BotAction> {
    vec![
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: format!(
                "Agregado al pedido.\n\nResumen parcial:\n{}",
                partial_summary(&context.items)
            ),
        },
        BotAction::SendButtons {
            to: context.phone_number.clone(),
            body: "¿Deseas agregar más granizados?".to_string(),
            buttons: vec![
                reply_button(ADD_MORE, "Agregar más"),
                reply_button(FINISH_ORDER, "Finalizar pedido"),
            ],
        },
    ]
}

pub fn validate_quantity(input: &str) -> Result<u32, String> {
    let quantity = input
        .trim()
        .parse::<u32>()
        .map_err(|_| "La cantidad debe ser un número entero.".to_string())?;

    if !(1..=999).contains(&quantity) {
        return Err("La cantidad debe estar entre 1 y 999.".to_string());
    }

    Ok(quantity)
}

pub fn normalize_flavor(input: &str) -> String {
    input
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn partial_summary(items: &[OrderItemData]) -> String {
    items
        .iter()
        .map(|item| {
            format!(
                "- {} x {} ({})",
                item.quantity,
                item.flavor,
                if item.has_liquor {
                    "con licor"
                } else {
                    "sin licor"
                }
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
        handle_add_more, handle_select_flavor, handle_select_quantity, handle_select_type,
        normalize_flavor, validate_quantity,
    };

    fn context() -> ConversationContext {
        ConversationContext {
            phone_number: "573001234567".to_string(),
            customer_name: None,
            customer_phone: None,
            delivery_address: None,
            items: Vec::new(),
            delivery_type: None,
            scheduled_date: None,
            scheduled_time: None,
            payment_method: None,
            receipt_media_id: None,
            current_order_id: None,
            editing_address: false,
            receipt_timer_expired: false,
            pending_has_liquor: None,
            pending_flavor: None,
        }
    }

    #[test]
    fn normalizes_flavor() {
        assert_eq!(
            normalize_flavor("  Maracuya   Especial "),
            "maracuya especial"
        );
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
    fn select_flavor_moves_to_quantity() {
        let mut context = context();
        let (state, _) = handle_select_flavor(
            &UserInput::TextMessage("Mora".to_string()),
            &mut context,
            false,
        )
        .expect("transition");

        assert_eq!(
            state,
            ConversationState::SelectQuantity {
                has_liquor: false,
                flavor: "mora".to_string()
            }
        );
        assert_eq!(context.pending_flavor.as_deref(), Some("mora"));
    }

    #[test]
    fn select_quantity_adds_item() {
        let mut context = context();
        let (state, _) = handle_select_quantity(
            &UserInput::TextMessage("3".to_string()),
            &mut context,
            true,
            "maracuya",
        )
        .expect("transition");

        assert_eq!(state, ConversationState::AddMore);
        assert_eq!(context.items.len(), 1);
        assert_eq!(context.items[0].quantity, 3);
    }

    #[test]
    fn add_more_finish_moves_to_summary() {
        let mut context = context();
        context.items.push(crate::db::models::OrderItemData {
            flavor: "mora".to_string(),
            has_liquor: false,
            quantity: 2,
        });

        let (state, _) = handle_add_more(
            &UserInput::ButtonPress("finish_order".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::ShowSummary);
    }
}
