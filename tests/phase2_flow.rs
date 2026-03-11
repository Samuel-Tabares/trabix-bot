use granizado_bot::{
    bot::state_machine::{
        transition, BotAction, ConversationContext, ConversationState, UserInput,
    },
    db::models::ConversationStateData,
};

fn base_context() -> ConversationContext {
    ConversationContext::from_persisted(
        "573001234567".to_string(),
        None,
        None,
        None,
        &ConversationStateData::default(),
    )
}

fn advance(
    state: ConversationState,
    input: UserInput,
    mut context: ConversationContext,
) -> (ConversationState, ConversationContext, Vec<BotAction>) {
    let (next_state, actions) = transition(&state, &input, &mut context).expect("transition");
    let stored_key = next_state.as_storage_key().to_string();
    let stored_data = context.to_state_data();
    let rehydrated_context = ConversationContext::from_persisted(
        context.phone_number.clone(),
        context.customer_name.clone(),
        context.customer_phone.clone(),
        context.delivery_address.clone(),
        &stored_data,
    );
    let rehydrated_state =
        ConversationState::from_storage_key(&stored_key, &rehydrated_context).expect("state");

    (rehydrated_state, rehydrated_context, actions)
}

fn contains_text(actions: &[BotAction], needle: &str) -> bool {
    actions.iter().any(|action| match action {
        BotAction::SendText { body, .. } => body.contains(needle),
        _ => false,
    })
}

fn list_row_count(actions: &[BotAction]) -> Option<usize> {
    actions.iter().find_map(|action| match action {
        BotAction::SendList { sections, .. } => Some(
            sections
                .iter()
                .map(|section| section.rows.len())
                .sum::<usize>(),
        ),
        _ => None,
    })
}

fn has_asset_image(actions: &[BotAction]) -> bool {
    actions.iter().any(|action| {
        matches!(
            action,
            BotAction::SendAssetImage { .. } | BotAction::SendImage { .. }
        )
    })
}

#[test]
fn greets_with_main_menu_list_and_four_options() {
    let (state, _context, actions) = advance(
        ConversationState::MainMenu,
        UserInput::TextMessage("hola".to_string()),
        base_context(),
    );

    assert_eq!(state, ConversationState::MainMenu);
    assert!(contains_text(&actions, "bienvenido"));
    assert_eq!(list_row_count(&actions), Some(4));
}

#[test]
fn navigates_view_menu_and_view_schedule() {
    let (state, context, actions) = advance(
        ConversationState::MainMenu,
        UserInput::ListSelection("view_menu".to_string()),
        base_context(),
    );
    assert_eq!(state, ConversationState::ViewMenu);
    assert!(contains_text(&actions, "MENÚ Y PRECIOS"));
    assert!(has_asset_image(&actions));

    let (state, _context, actions) = advance(
        state,
        UserInput::ButtonPress("back_main_menu".to_string()),
        context,
    );
    assert_eq!(state, ConversationState::MainMenu);
    assert_eq!(list_row_count(&actions), Some(4));

    let (state, _context, actions) = advance(
        ConversationState::MainMenu,
        UserInput::ListSelection("view_schedule".to_string()),
        base_context(),
    );
    assert_eq!(state, ConversationState::ViewSchedule);
    assert!(contains_text(&actions, "HORARIOS"));
}

#[test]
fn select_flavor_uses_lists_without_images() {
    let mut context = base_context();
    context.customer_name = Some("Ana Maria".to_string());
    context.customer_phone = Some("3001234567".to_string());
    context.delivery_address = Some("Cra 15 #20-30 Armenia".to_string());
    context.delivery_type = Some("immediate".to_string());

    let (state, _context, actions) = advance(
        ConversationState::SelectType,
        UserInput::ButtonPress("with_liquor".to_string()),
        context,
    );

    assert_eq!(state, ConversationState::SelectFlavor { has_liquor: true });
    assert_eq!(list_row_count(&actions), Some(7));
    assert!(!has_asset_image(&actions));
}

#[test]
fn validates_programmed_delivery_and_persists_across_restarts() {
    let (state, context, _) = advance(
        ConversationState::MainMenu,
        UserInput::ListSelection("make_order".to_string()),
        base_context(),
    );
    assert_eq!(state, ConversationState::WhenDelivery);

    let (state, context, _) = advance(
        state,
        UserInput::ButtonPress("scheduled_delivery".to_string()),
        context,
    );
    assert_eq!(state, ConversationState::SelectDate);

    let (state, context, actions) = advance(
        state.clone(),
        UserInput::TextMessage("x".to_string()),
        context.clone(),
    );
    assert_eq!(state, ConversationState::SelectDate);
    assert!(contains_text(&actions, "fecha"));

    let (state, context, _) = advance(
        state,
        UserInput::TextMessage("2030-12-24".to_string()),
        context,
    );
    assert_eq!(state, ConversationState::SelectTime);
    assert_eq!(context.scheduled_date.as_deref(), Some("2030-12-24"));

    let (state, context, actions) = advance(
        state.clone(),
        UserInput::TextMessage("".to_string()),
        context.clone(),
    );
    assert_eq!(state, ConversationState::SelectTime);
    assert!(contains_text(&actions, "hora"));

    let (state, context, _) = advance(state, UserInput::TextMessage("3:45pm".to_string()), context);
    assert_eq!(state, ConversationState::ConfirmSchedule);
    assert_eq!(context.scheduled_time.as_deref(), Some("3:45pm"));

    let (state, context, _) = advance(
        state,
        UserInput::ButtonPress("confirm_schedule".to_string()),
        context,
    );
    assert_eq!(state, ConversationState::CollectName);
    assert_eq!(context.delivery_type.as_deref(), Some("scheduled"));
    assert_eq!(context.scheduled_date.as_deref(), Some("2030-12-24"));
    assert_eq!(context.scheduled_time.as_deref(), Some("3:45pm"));
}

#[test]
fn immediate_delivery_keeps_advancing_to_data_collection() {
    let (state, context, _) = advance(
        ConversationState::MainMenu,
        UserInput::ListSelection("make_order".to_string()),
        base_context(),
    );

    let (state, context, actions) = advance(
        state,
        UserInput::ButtonPress("immediate_delivery".to_string()),
        context,
    );

    assert!(matches!(
        state,
        ConversationState::CollectName | ConversationState::OutOfHours
    ));
    assert_eq!(context.delivery_type.as_deref(), Some("immediate"));
    assert!(!actions.is_empty());
}

#[test]
fn collects_customer_data_with_retries() {
    let (state, context, actions) = advance(
        ConversationState::CollectName,
        UserInput::TextMessage("A".to_string()),
        base_context(),
    );
    assert_eq!(state, ConversationState::CollectName);
    assert!(contains_text(&actions, "nombre"));

    let (state, context, _) = advance(
        state,
        UserInput::TextMessage("Ana Maria".to_string()),
        context,
    );
    assert_eq!(state, ConversationState::CollectPhone);
    assert_eq!(context.customer_name.as_deref(), Some("Ana Maria"));

    let (state, context, actions) =
        advance(state, UserInput::TextMessage("abc123".to_string()), context);
    assert_eq!(state, ConversationState::CollectPhone);
    assert!(contains_text(&actions, "dígitos"));

    let (state, context, _) = advance(
        state,
        UserInput::TextMessage("3001234567".to_string()),
        context,
    );
    assert_eq!(state, ConversationState::CollectAddress);
    assert_eq!(context.customer_phone.as_deref(), Some("3001234567"));

    let (state, context, actions) =
        advance(state, UserInput::TextMessage("abc".to_string()), context);
    assert_eq!(state, ConversationState::CollectAddress);
    assert!(contains_text(&actions, "dirección"));

    let (state, context, _) = advance(
        state,
        UserInput::TextMessage("Cra 15 #20-30 Armenia".to_string()),
        context,
    );
    assert_eq!(state, ConversationState::SelectType);
    assert_eq!(
        context.delivery_address.as_deref(),
        Some("Cra 15 #20-30 Armenia")
    );
}

#[test]
fn supports_mixed_items_loop_and_reaches_show_summary() {
    let mut context = base_context();
    context.customer_name = Some("Ana Maria".to_string());
    context.customer_phone = Some("3001234567".to_string());
    context.delivery_address = Some("Cra 15 #20-30 Armenia".to_string());
    context.delivery_type = Some("immediate".to_string());

    let (state, context, _) = advance(
        ConversationState::SelectType,
        UserInput::ButtonPress("with_liquor".to_string()),
        context,
    );
    assert_eq!(state, ConversationState::SelectFlavor { has_liquor: true });

    let (state, context, _) = advance(
        state,
        UserInput::ListSelection("liquor_maracumango_ron_blanco".to_string()),
        context,
    );
    assert_eq!(
        state,
        ConversationState::SelectQuantity {
            has_liquor: true,
            flavor: "Maracumango Ron blanco".to_string()
        }
    );

    let (state, context, actions) =
        advance(state, UserInput::TextMessage("0".to_string()), context);
    assert!(matches!(state, ConversationState::SelectQuantity { .. }));
    assert!(contains_text(&actions, "cantidad"));

    let (state, context, actions) =
        advance(state, UserInput::TextMessage("2".to_string()), context);
    assert_eq!(state, ConversationState::AddMore);
    assert!(contains_text(&actions, "Resumen parcial"));

    let (state, context, _) = advance(
        state,
        UserInput::ButtonPress("add_more".to_string()),
        context,
    );
    assert_eq!(state, ConversationState::SelectType);

    let (state, context, _) = advance(
        state,
        UserInput::ButtonPress("without_liquor".to_string()),
        context,
    );
    let (state, context, _) = advance(
        state,
        UserInput::ListSelection("non_liquor_bonbonbum".to_string()),
        context,
    );
    let (state, context, _) = advance(state, UserInput::TextMessage("1".to_string()), context);
    assert_eq!(state, ConversationState::AddMore);

    let (state, context, _) = advance(
        state,
        UserInput::ButtonPress("add_more".to_string()),
        context,
    );
    let (state, context, _) = advance(
        state,
        UserInput::ButtonPress("with_liquor".to_string()),
        context,
    );
    let (state, context, _) = advance(
        state,
        UserInput::ListSelection("liquor_blueberry_vodka".to_string()),
        context,
    );
    let (state, context, _) = advance(state, UserInput::TextMessage("4".to_string()), context);
    assert_eq!(state, ConversationState::AddMore);
    assert_eq!(context.items.len(), 3);
    assert!(context.items.iter().any(|item| item.has_liquor));
    assert!(context.items.iter().any(|item| !item.has_liquor));

    let (state, context, actions) = advance(
        state,
        UserInput::ButtonPress("finish_order".to_string()),
        context,
    );
    assert_eq!(state, ConversationState::ShowSummary);
    assert!(contains_text(&actions, "Total estimado"));
    assert!(contains_text(&actions, "Maracumango Ron blanco"));
    assert!(contains_text(&actions, "Bonbonbum"));
    assert!(contains_text(&actions, "Blueberry Vodka"));
    assert_eq!(list_row_count(&actions), Some(4));

    let persisted = context.to_state_data();
    assert_eq!(persisted.items.len(), 3);

    let (state, context, actions) = advance(
        state,
        UserInput::ListSelection("cancel_order".to_string()),
        context,
    );
    assert_eq!(state, ConversationState::MainMenu);
    assert!(actions
        .iter()
        .any(|action| matches!(action, BotAction::ResetConversation { .. })));
    assert_eq!(list_row_count(&actions), Some(4));
    assert!(context.items.is_empty());
}
