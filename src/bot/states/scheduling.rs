use chrono::{DateTime, FixedOffset, NaiveTime, Utc};

use crate::{
    bot::state_machine::BotAction,
    messages::client_messages,
    whatsapp::types::{Button, ButtonReplyPayload},
};

const IMMEDIATE_DELIVERY: &str = "immediate_delivery";
const SCHEDULED_DELIVERY: &str = "scheduled_delivery";

const BUSINESS_HOURS_START_HOUR: u32 = 8;
const BUSINESS_HOURS_END_HOUR: u32 = 23;

pub fn when_delivery_actions(phone: &str) -> Vec<BotAction> {
    let messages = &client_messages().scheduling;
    vec![BotAction::SendButtons {
        to: phone.to_string(),
        body: messages.when_delivery_body.clone(),
        buttons: vec![
            reply_button(IMMEDIATE_DELIVERY, &messages.immediate_button),
            reply_button(SCHEDULED_DELIVERY, &messages.scheduled_button),
        ],
    }]
}

pub fn select_date_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages().scheduling.select_date_prompt.clone(),
    }]
}

pub fn is_within_business_hours(time: NaiveTime) -> bool {
    let start = business_hours_start();
    let end = business_hours_end();
    time >= start && time <= end
}

pub fn immediate_delivery_hours_text() -> String {
    format!(
        "{} - {}",
        business_hours_start().format("%-I:%M %p"),
        business_hours_end().format("%-I:%M %p")
    )
}

fn business_hours_start() -> NaiveTime {
    NaiveTime::from_hms_opt(BUSINESS_HOURS_START_HOUR, 0, 0).expect("static time")
}

fn business_hours_end() -> NaiveTime {
    NaiveTime::from_hms_opt(BUSINESS_HOURS_END_HOUR, 0, 0).expect("static time")
}

fn now_bogota() -> DateTime<FixedOffset> {
    let offset = FixedOffset::west_opt(5 * 3600).expect("valid offset");
    Utc::now().with_timezone(&offset)
}

pub fn current_bogota_now() -> DateTime<FixedOffset> {
    now_bogota()
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
    use chrono::NaiveTime;

    use super::is_within_business_hours;

    #[test]
    fn validates_business_hours() {
        assert!(is_within_business_hours(
            NaiveTime::from_hms_opt(8, 0, 0).unwrap()
        ));
        assert!(!is_within_business_hours(
            NaiveTime::from_hms_opt(7, 59, 0).unwrap()
        ));
    }
}
