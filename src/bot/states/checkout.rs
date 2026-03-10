use crate::bot::state_machine::{
    BotAction, ConversationContext, ConversationState, TransitionResult, UserInput,
};

use super::menu;

pub fn handle_show_summary(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    if wants_main_menu(input) {
        return Ok((
            ConversationState::MainMenu,
            {
                let mut actions = vec![BotAction::ResetConversation {
                    phone: context.phone_number.clone(),
                }];
                actions.extend(menu::main_menu_actions(&context.phone_number));
                actions
            },
        ));
    }

    Ok((
        ConversationState::ShowSummary,
        show_summary_actions(context),
    ))
}

pub fn handle_order_complete(context: &mut ConversationContext) -> TransitionResult {
    let mut actions = vec![BotAction::ResetConversation {
        phone: context.phone_number.clone(),
    }];
    actions.extend(menu::main_menu_actions(&context.phone_number));

    Ok((ConversationState::MainMenu, actions))
}

pub fn show_summary_actions(context: &ConversationContext) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: context.phone_number.clone(),
        body: format!(
            "RESUMEN DEL PEDIDO\n\nCliente: {}\nTeléfono: {}\nDirección: {}\nEntrega: {}\nFecha: {}\nHora: {}\n\nItems:\n{}\n\nEn Fase 2 este resumen es provisional. Los precios, pagos y confirmaciones se implementan en Fase 3.\n\nSi quieres volver al menú principal, escribe 'menu'.",
            context.customer_name.as_deref().unwrap_or("pendiente"),
            context.customer_phone.as_deref().unwrap_or("pendiente"),
            context.delivery_address.as_deref().unwrap_or("pendiente"),
            context.delivery_type.as_deref().unwrap_or("pendiente"),
            context.scheduled_date.as_deref().unwrap_or("no aplica"),
            context.scheduled_time.as_deref().unwrap_or("no aplica"),
            items_summary(&context.items),
        ),
    }]
}

fn items_summary(items: &[crate::db::models::OrderItemData]) -> String {
    if items.is_empty() {
        return "- Sin items".to_string();
    }

    items.iter()
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

fn wants_main_menu(input: &UserInput) -> bool {
    match input {
        UserInput::TextMessage(text) => text.trim().eq_ignore_ascii_case("menu"),
        UserInput::ButtonPress(id) | UserInput::ListSelection(id) => id == "back_main_menu",
        _ => false,
    }
}

