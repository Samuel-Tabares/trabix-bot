use crate::{
    bot::state_machine::{
        BotAction, ConversationContext, ConversationState, TransitionResult, UserInput,
    },
    whatsapp::types::{Button, ButtonReplyPayload, ListRow, ListSection},
};

use super::{advisor, scheduling};

const MAKE_ORDER: &str = "make_order";
const VIEW_MENU: &str = "view_menu";
const VIEW_SCHEDULE: &str = "view_schedule";
const CONTACT_ADVISOR: &str = "contact_advisor";
const BACK_MAIN_MENU: &str = "back_main_menu";

pub fn handle_main_menu(input: &UserInput, context: &mut ConversationContext) -> TransitionResult {
    let selection = selection_id(input);

    match selection.as_deref() {
        Some(MAKE_ORDER) => Ok((
            ConversationState::WhenDelivery,
            scheduling::when_delivery_actions(&context.phone_number),
        )),
        Some(VIEW_MENU) => Ok((
            ConversationState::ViewMenu,
            view_menu_actions(&context.phone_number),
        )),
        Some(VIEW_SCHEDULE) => Ok((
            ConversationState::ViewSchedule,
            view_schedule_actions(&context.phone_number),
        )),
        Some(CONTACT_ADVISOR) => Ok((
            ConversationState::ContactAdvisorName,
            advisor::contact_advisor_name_actions(&context.phone_number),
        )),
        _ => Ok((
            ConversationState::MainMenu,
            main_menu_actions(&context.phone_number),
        )),
    }
}

pub fn handle_view_menu(input: &UserInput, context: &mut ConversationContext) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(MAKE_ORDER) => Ok((
            ConversationState::WhenDelivery,
            scheduling::when_delivery_actions(&context.phone_number),
        )),
        Some(BACK_MAIN_MENU) => Ok((
            ConversationState::MainMenu,
            main_menu_actions(&context.phone_number),
        )),
        _ => Ok((
            ConversationState::ViewMenu,
            with_retry_message(
                "Selecciona una opción del menú para continuar.",
                view_menu_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn handle_view_schedule(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(MAKE_ORDER) => Ok((
            ConversationState::WhenDelivery,
            scheduling::when_delivery_actions(&context.phone_number),
        )),
        Some(BACK_MAIN_MENU) => Ok((
            ConversationState::MainMenu,
            main_menu_actions(&context.phone_number),
        )),
        _ => Ok((
            ConversationState::ViewSchedule,
            with_retry_message(
                "Selecciona una opción válida para continuar.",
                view_schedule_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn main_menu_actions(phone: &str) -> Vec<BotAction> {
    vec![
        BotAction::SendText {
            to: phone.to_string(),
            body: "Hola, bienvenido a Granizados. Elige una opción del menú principal.".to_string(),
        },
        BotAction::SendList {
            to: phone.to_string(),
            body: "¿Qué deseas hacer?".to_string(),
            button_text: "Ver opciones".to_string(),
            sections: vec![ListSection {
                title: "Menú Principal".to_string(),
                rows: vec![
                    ListRow {
                        id: MAKE_ORDER.to_string(),
                        title: "Hacer Pedido".to_string(),
                        description: "Arma tu pedido de granizados".to_string(),
                    },
                    ListRow {
                        id: VIEW_MENU.to_string(),
                        title: "Ver Menú".to_string(),
                        description: "Sabores y precios".to_string(),
                    },
                    ListRow {
                        id: VIEW_SCHEDULE.to_string(),
                        title: "Horarios".to_string(),
                        description: "Horarios de entrega".to_string(),
                    },
                    ListRow {
                        id: CONTACT_ADVISOR.to_string(),
                        title: "Hablar con Asesor".to_string(),
                        description: "Atención por asesor".to_string(),
                    },
                ],
            }],
        },
    ]
}

pub fn view_menu_actions(phone: &str) -> Vec<BotAction> {
    vec![
        BotAction::SendText {
            to: phone.to_string(),
            body: "MENÚ Y PRECIOS\n\nDETAL:\nCon licor: $8.000\nSegundo con licor: $4.000\nSin licor: $7.000 c/u\n\nAL MAYOR (20+ del mismo tipo):\nCon licor: 20-49u $4.900 | 50-99u $4.700 | 100+u $4.500\nSin licor: 20-49u $4.800 | 50-99u $4.500 | 100+u $4.200\n\nEn Fase 2 el menú visual se envía como texto provisional.".to_string(),
        },
        BotAction::SendButtons {
            to: phone.to_string(),
            body: "¿Qué deseas hacer ahora?".to_string(),
            buttons: vec![
                reply_button(MAKE_ORDER, "Hacer Pedido"),
                reply_button(BACK_MAIN_MENU, "Volver al Menú"),
            ],
        },
    ]
}

pub fn view_schedule_actions(phone: &str) -> Vec<BotAction> {
    vec![
        BotAction::SendText {
            to: phone.to_string(),
            body: "HORARIOS\nEntrega inmediata: 8:00 AM - 11:00 PM\n\nSi estás fuera de este horario, aún podemos intentar programar tu pedido o dejarlo listo para asesor.".to_string(),
        },
        BotAction::SendButtons {
            to: phone.to_string(),
            body: "¿Deseas hacer un pedido?".to_string(),
            buttons: vec![
                reply_button(MAKE_ORDER, "Hacer Pedido"),
                reply_button(BACK_MAIN_MENU, "Volver al Menú"),
            ],
        },
    ]
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

fn with_retry_message(message: &str, mut actions: Vec<BotAction>) -> Vec<BotAction> {
    let mut all = vec![BotAction::SendText {
        to: target_phone(&actions),
        body: message.to_string(),
    }];
    all.append(&mut actions);
    all
}

fn target_phone(actions: &[BotAction]) -> String {
    actions
        .iter()
        .find_map(|action| match action {
            BotAction::SendText { to, .. }
            | BotAction::SendButtons { to, .. }
            | BotAction::SendList { to, .. }
            | BotAction::SendImage { to, .. } => Some(to.clone()),
            _ => None,
        })
        .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use crate::bot::state_machine::{ConversationContext, ConversationState, UserInput};

    use super::{handle_main_menu, handle_view_menu, handle_view_schedule};

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
    fn main_menu_free_text_shows_menu() {
        let mut context = context();
        let (state, actions) =
            handle_main_menu(&UserInput::TextMessage("hola".to_string()), &mut context)
                .expect("transition");

        assert_eq!(state, ConversationState::MainMenu);
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn main_menu_make_order_moves_forward() {
        let mut context = context();
        let (state, _) = handle_main_menu(
            &UserInput::ListSelection("make_order".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::WhenDelivery);
    }

    #[test]
    fn view_menu_back_returns_to_main_menu() {
        let mut context = context();
        let (state, _) = handle_view_menu(
            &UserInput::ButtonPress("back_main_menu".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::MainMenu);
    }

    #[test]
    fn view_schedule_make_order_moves_forward() {
        let mut context = context();
        let (state, _) = handle_view_schedule(
            &UserInput::ButtonPress("make_order".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::WhenDelivery);
    }
}
