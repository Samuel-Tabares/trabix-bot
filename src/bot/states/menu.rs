use crate::{
    messages::client_messages,
    bot::state_machine::{
        BotAction, ConversationContext, ConversationState, ImageAsset, TransitionResult, UserInput,
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
        Some(CONTACT_ADVISOR) => {
            let (state, actions) = advisor::start_contact_advisor(context);
            Ok((state, actions))
        }
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
                &client_messages().menu.retry_view_menu,
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
                &client_messages().menu.retry_view_schedule,
                view_schedule_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn main_menu_actions(phone: &str) -> Vec<BotAction> {
    let messages = &client_messages().menu;
    vec![
        BotAction::SendText {
            to: phone.to_string(),
            body: messages.main_welcome.clone(),
        },
        BotAction::SendList {
            to: phone.to_string(),
            body: messages.main_list_body.clone(),
            button_text: messages.main_list_button_text.clone(),
            sections: vec![ListSection {
                title: messages.main_section_title.clone(),
                rows: vec![
                    ListRow {
                        id: MAKE_ORDER.to_string(),
                        title: messages.make_order_title.clone(),
                        description: messages.make_order_description.clone(),
                    },
                    ListRow {
                        id: VIEW_MENU.to_string(),
                        title: messages.view_menu_title.clone(),
                        description: messages.view_menu_description.clone(),
                    },
                    ListRow {
                        id: VIEW_SCHEDULE.to_string(),
                        title: messages.view_schedule_title.clone(),
                        description: messages.view_schedule_description.clone(),
                    },
                    ListRow {
                        id: CONTACT_ADVISOR.to_string(),
                        title: messages.contact_advisor_title.clone(),
                        description: messages.contact_advisor_description.clone(),
                    },
                ],
            }],
        },
    ]
}

pub fn view_menu_actions(phone: &str) -> Vec<BotAction> {
    let messages = &client_messages().menu;
    vec![
        BotAction::SendAssetImage {
            to: phone.to_string(),
            asset: ImageAsset::Menu,
            caption: Some(messages.menu_image_caption.clone()),
        },
        BotAction::SendText {
            to: phone.to_string(),
            body: messages.menu_text.clone(),
        },
        BotAction::SendButtons {
            to: phone.to_string(),
            body: messages.view_menu_buttons_body.clone(),
            buttons: vec![
                reply_button(MAKE_ORDER, &messages.view_menu_make_order_button),
                reply_button(BACK_MAIN_MENU, &messages.view_menu_back_button),
            ],
        },
    ]
}

pub fn view_schedule_actions(phone: &str) -> Vec<BotAction> {
    let messages = &client_messages().menu;
    vec![
        BotAction::SendText {
            to: phone.to_string(),
            body: messages.schedule_text.clone(),
        },
        BotAction::SendButtons {
            to: phone.to_string(),
            body: messages.schedule_buttons_body.clone(),
            buttons: vec![
                reply_button(MAKE_ORDER, &messages.schedule_make_order_button),
                reply_button(BACK_MAIN_MENU, &messages.schedule_back_button),
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
            | BotAction::SendImage { to, .. }
            | BotAction::SendAssetImage { to, .. } => Some(to.clone()),
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
            advisor_phone: "573009999999".to_string(),
            customer_name: None,
            customer_phone: None,
            delivery_address: None,
            items: Vec::new(),
            delivery_type: None,
            scheduled_date: None,
            scheduled_time: None,
            payment_method: None,
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
