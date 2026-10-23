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

/// La próxima apertura (hoy a las 8:00 si todavía no abrimos, si no mañana).
pub fn next_business_opening_utc(now: DateTime<Utc>) -> DateTime<Utc> {
    let offset = FixedOffset::west_opt(5 * 3600).expect("valid offset");
    let local = now.with_timezone(&offset);
    let mut date = local.date_naive();
    if local.time() >= business_hours_start() {
        date = date.succ_opt().expect("valid date");
    }
    date.and_time(business_hours_start())
        .and_local_timezone(offset)
        .single()
        .expect("fixed offset is unambiguous")
        .with_timezone(&Utc)
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
    use chrono::{NaiveTime, TimeZone, Utc};

    use super::{is_within_business_hours, next_business_opening_utc};

    #[test]
    fn next_opening_is_the_same_morning_after_midnight_and_tomorrow_late_at_night() {
        // 23:05 Bogotá del 19 = 04:05 UTC del 20 → abre el 20 a las 8:00 (13:00 UTC).
        let late_night = Utc.with_ymd_and_hms(2026, 10, 20, 4, 5, 0).unwrap();
        assert_eq!(
            next_business_opening_utc(late_night),
            Utc.with_ymd_and_hms(2026, 10, 20, 13, 0, 0).unwrap()
        );
        // 3:00 Bogotá del 20 = 08:00 UTC → mismo día a las 8:00.
        let early = Utc.with_ymd_and_hms(2026, 10, 20, 8, 0, 0).unwrap();
        assert_eq!(
            next_business_opening_utc(early),
            Utc.with_ymd_and_hms(2026, 10, 20, 13, 0, 0).unwrap()
        );
        // 23:30 Bogotá del 20 = 04:30 UTC del 21 → el 21 a las 8:00.
        let after_close = Utc.with_ymd_and_hms(2026, 10, 21, 4, 30, 0).unwrap();
        assert_eq!(
            next_business_opening_utc(after_close),
            Utc.with_ymd_and_hms(2026, 10, 21, 13, 0, 0).unwrap()
        );
    }

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
