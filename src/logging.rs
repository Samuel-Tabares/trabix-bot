use crate::bot::state_machine::{BotAction, ImageAsset};

const MAX_PREVIEW_CHARS: usize = 80;

pub fn mask_phone(phone: &str) -> String {
    if phone.len() <= 4 {
        phone.to_string()
    } else {
        format!("...{}", &phone[phone.len() - 4..])
    }
}

pub fn preview_text(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return "<empty>".to_string();
    }

    let mut preview = String::new();
    for (index, ch) in collapsed.chars().enumerate() {
        if index >= MAX_PREVIEW_CHARS {
            preview.push_str("...");
            break;
        }
        preview.push(ch);
    }

    preview
}

pub fn summarize_action_kinds(actions: &[BotAction]) -> String {
    let mut kinds = actions.iter().map(action_kind).collect::<Vec<_>>();
    if kinds.len() > 5 {
        let remainder = kinds.len() - 5;
        kinds.truncate(5);
        return format!("{},+{remainder} more", kinds.join(","));
    }

    kinds.join(",")
}

pub fn action_kind(action: &BotAction) -> &'static str {
    match action {
        BotAction::SendText { .. } => "send_text",
        BotAction::SendButtons { .. } => "send_buttons",
        BotAction::SendList { .. } => "send_list",
        BotAction::SendImage { .. } => "send_image",
        BotAction::SendAssetImage { .. } => "send_asset_image",
        BotAction::SendTransferInstructions { .. } => "send_transfer_instructions",
        BotAction::StartTimer { .. } => "start_timer",
        BotAction::CancelTimer { .. } => "cancel_timer",
        BotAction::UpsertDraftOrder { .. } => "upsert_draft_order",
        BotAction::FinalizeCurrentOrder { .. } => "finalize_current_order",
        BotAction::UpdateCurrentOrderDeliveryCost { .. } => "update_current_order_delivery_cost",
        BotAction::CancelCurrentOrder { .. } => "cancel_current_order",
        BotAction::SaveOrder { .. } => "save_order",
        BotAction::BindAdvisorSession { .. } => "bind_advisor_session",
        BotAction::ClearAdvisorSession { .. } => "clear_advisor_session",
        BotAction::ResetConversation { .. } => "reset_conversation",
        BotAction::RelayMessage { .. } => "relay_message",
        BotAction::NoOp => "no_op",
    }
}

pub fn log_bot_action(action: &BotAction) {
    match action {
        BotAction::SendText { to, body } => {
            tracing::info!(
                recipient = %mask_phone(to),
                action = "send_text",
                preview = %preview_text(body),
                "dispatching bot action"
            );
        }
        BotAction::SendButtons { to, body, buttons } => {
            tracing::info!(
                recipient = %mask_phone(to),
                action = "send_buttons",
                button_count = buttons.len(),
                preview = %preview_text(body),
                "dispatching bot action"
            );
        }
        BotAction::SendList {
            to, body, sections, ..
        } => {
            tracing::info!(
                recipient = %mask_phone(to),
                action = "send_list",
                section_count = sections.len(),
                row_count = sections.iter().map(|section| section.rows.len()).sum::<usize>(),
                preview = %preview_text(body),
                "dispatching bot action"
            );
        }
        BotAction::SendImage {
            to,
            media_id,
            caption,
        } => {
            tracing::info!(
                recipient = %mask_phone(to),
                action = "send_image",
                media_id = %preview_text(media_id),
                caption = %caption
                    .as_deref()
                    .map(preview_text)
                    .unwrap_or_else(|| "<none>".to_string()),
                "dispatching bot action"
            );
        }
        BotAction::SendAssetImage { to, asset, caption } => {
            tracing::info!(
                recipient = %mask_phone(to),
                action = "send_asset_image",
                asset = %asset_label(asset),
                caption = %caption
                    .as_deref()
                    .map(preview_text)
                    .unwrap_or_else(|| "<none>".to_string()),
                "dispatching bot action"
            );
        }
        BotAction::SendTransferInstructions { to } => {
            tracing::info!(
                recipient = %mask_phone(to),
                action = "send_transfer_instructions",
                "dispatching bot action"
            );
        }
        BotAction::StartTimer {
            timer_type,
            phone,
            duration,
        } => {
            tracing::info!(
                phone = %mask_phone(phone),
                action = "start_timer",
                timer_type = %timer_type.as_str(),
                duration_secs = duration.as_secs(),
                "dispatching bot action"
            );
        }
        BotAction::CancelTimer { timer_type, phone } => {
            tracing::info!(
                phone = %mask_phone(phone),
                action = "cancel_timer",
                timer_type = %timer_type.as_str(),
                "dispatching bot action"
            );
        }
        BotAction::UpsertDraftOrder { status } => {
            tracing::info!(action = "upsert_draft_order", status = %status, "dispatching bot action");
        }
        BotAction::FinalizeCurrentOrder { status } => {
            tracing::info!(action = "finalize_current_order", status = %status, "dispatching bot action");
        }
        BotAction::UpdateCurrentOrderDeliveryCost {
            delivery_cost,
            total_final,
            status,
        } => {
            tracing::info!(
                action = "update_current_order_delivery_cost",
                delivery_cost = *delivery_cost,
                total_final = *total_final,
                status = %status,
                "dispatching bot action"
            );
        }
        BotAction::CancelCurrentOrder { order_id } => {
            tracing::info!(
                action = "cancel_current_order",
                order_id = *order_id,
                "dispatching bot action"
            );
        }
        BotAction::SaveOrder { .. } => {
            tracing::debug!(action = "save_order", "dispatching bot action");
        }
        BotAction::BindAdvisorSession {
            advisor_phone,
            target_phone,
        } => {
            tracing::info!(
                action = "bind_advisor_session",
                advisor_phone = %mask_phone(advisor_phone),
                target_phone = %mask_phone(target_phone),
                "dispatching bot action"
            );
        }
        BotAction::ClearAdvisorSession { advisor_phone } => {
            tracing::info!(
                action = "clear_advisor_session",
                advisor_phone = %mask_phone(advisor_phone),
                "dispatching bot action"
            );
        }
        BotAction::ResetConversation { phone } => {
            tracing::info!(
                action = "reset_conversation",
                phone = %mask_phone(phone),
                "dispatching bot action"
            );
        }
        BotAction::RelayMessage { to, body, .. } => {
            tracing::info!(
                recipient = %mask_phone(to),
                action = "relay_message",
                preview = %preview_text(body),
                "dispatching bot action"
            );
        }
        BotAction::NoOp => {
            tracing::debug!(action = "no_op", "dispatching bot action");
        }
    }
}

fn asset_label(asset: &ImageAsset) -> &'static str {
    match asset {
        ImageAsset::Menu => "menu",
    }
}

#[cfg(test)]
mod tests {
    use crate::bot::state_machine::{BotAction, TimerType};

    use super::{action_kind, mask_phone, preview_text, summarize_action_kinds};

    #[test]
    fn masks_phone_to_last_four_digits() {
        assert_eq!(mask_phone("573001234567"), "...4567");
        assert_eq!(mask_phone("1234"), "1234");
    }

    #[test]
    fn preview_text_collapses_spaces_and_truncates() {
        let preview = preview_text(" hola   mundo   desde    el    bot ");
        assert_eq!(preview, "hola mundo desde el bot");

        let long = "a".repeat(100);
        assert_eq!(preview_text(&long), format!("{}...", "a".repeat(80)));
    }

    #[test]
    fn summarizes_action_kinds_with_remainder() {
        let actions = vec![
            BotAction::SendText {
                to: "1".to_string(),
                body: "a".to_string(),
            },
            BotAction::SendButtons {
                to: "1".to_string(),
                body: "b".to_string(),
                buttons: vec![],
            },
            BotAction::StartTimer {
                timer_type: TimerType::AdvisorResponse,
                phone: "1".to_string(),
                duration: std::time::Duration::from_secs(1),
            },
            BotAction::CancelTimer {
                timer_type: TimerType::AdvisorResponse,
                phone: "1".to_string(),
            },
            BotAction::ResetConversation {
                phone: "1".to_string(),
            },
            BotAction::NoOp,
        ];

        assert_eq!(action_kind(&actions[0]), "send_text");
        assert_eq!(
            summarize_action_kinds(&actions),
            "send_text,send_buttons,start_timer,cancel_timer,reset_conversation,+1 more"
        );
    }
}
