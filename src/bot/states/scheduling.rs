use chrono::{FixedOffset, NaiveDate, NaiveTime, Utc};

use crate::{
    bot::{
        state_machine::{
            BotAction, ConversationContext, ConversationState, TransitionResult, UserInput,
        },
        states::{advisor, data_collect, menu},
    },
    whatsapp::types::{Button, ButtonReplyPayload},
};

const IMMEDIATE_DELIVERY: &str = "immediate_delivery";
const SCHEDULED_DELIVERY: &str = "scheduled_delivery";
const SCHEDULE_LATER: &str = "schedule_later";
const CONTACT_ADVISOR_NOW: &str = "contact_advisor_now";
const BACK_MAIN_MENU: &str = "back_main_menu";
const CONFIRM_SCHEDULE: &str = "confirm_schedule";
const CHANGE_SCHEDULE: &str = "change_schedule";

pub fn handle_when_delivery(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(IMMEDIATE_DELIVERY) => {
            context.delivery_type = Some("immediate".to_string());
            context.scheduled_date = None;
            context.scheduled_time = None;
            handle_check_schedule(context)
        }
        Some(SCHEDULED_DELIVERY) => {
            context.delivery_type = Some("scheduled".to_string());
            context.scheduled_date = None;
            context.scheduled_time = None;
            Ok((
                ConversationState::SelectDate,
                select_date_actions(&context.phone_number),
            ))
        }
        _ => Ok((
            ConversationState::WhenDelivery,
            retry_actions(
                &context.phone_number,
                "Selecciona si tu pedido es inmediato o programado.",
                when_delivery_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn handle_check_schedule(context: &mut ConversationContext) -> TransitionResult {
    if is_within_business_hours(now_bogota().time()) {
        Ok((
            ConversationState::CollectName,
            data_collect::collect_name_actions(&context.phone_number),
        ))
    } else {
        Ok((
            ConversationState::OutOfHours,
            out_of_hours_actions(&context.phone_number),
        ))
    }
}

pub fn handle_out_of_hours(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(SCHEDULE_LATER) => Ok((
            ConversationState::SelectDate,
            select_date_actions(&context.phone_number),
        )),
        Some(CONTACT_ADVISOR_NOW) => Ok((
            ConversationState::ContactAdvisorName,
            advisor::contact_advisor_name_actions(&context.phone_number),
        )),
        Some(BACK_MAIN_MENU) => Ok((
            ConversationState::MainMenu,
            menu::main_menu_actions(&context.phone_number),
        )),
        _ => Ok((
            ConversationState::OutOfHours,
            retry_actions(
                &context.phone_number,
                "Selecciona una de las opciones disponibles.",
                out_of_hours_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn handle_select_date(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match parse_future_date(text, now_bogota().date_naive()) {
            Ok(date) => {
                context.scheduled_date = Some(date.format("%Y-%m-%d").to_string());
                Ok((
                    ConversationState::SelectTime,
                    select_time_actions(&context.phone_number),
                ))
            }
            Err(message) => Ok((
                ConversationState::SelectDate,
                retry_actions(&context.phone_number, &message, select_date_actions(&context.phone_number)),
            )),
        },
        _ => Ok((
            ConversationState::SelectDate,
            retry_actions(
                &context.phone_number,
                "Escribe una fecha válida para programar tu pedido.",
                select_date_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn handle_select_time(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match input {
        UserInput::TextMessage(text) => match parse_time(text) {
            Ok(time) => {
                context.scheduled_time = Some(time.format("%H:%M").to_string());
                Ok((
                    ConversationState::ConfirmSchedule,
                    confirm_schedule_actions(context),
                ))
            }
            Err(message) => Ok((
                ConversationState::SelectTime,
                retry_actions(&context.phone_number, &message, select_time_actions(&context.phone_number)),
            )),
        },
        _ => Ok((
            ConversationState::SelectTime,
            retry_actions(
                &context.phone_number,
                "Escribe una hora válida. Ejemplo: 15:30 o 3:30pm.",
                select_time_actions(&context.phone_number),
            ),
        )),
    }
}

pub fn handle_confirm_schedule(
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match selection_id(input).as_deref() {
        Some(CONFIRM_SCHEDULE) => Ok((
            ConversationState::CollectName,
            data_collect::collect_name_actions(&context.phone_number),
        )),
        Some(CHANGE_SCHEDULE) => {
            context.scheduled_date = None;
            context.scheduled_time = None;
            Ok((
                ConversationState::SelectDate,
                select_date_actions(&context.phone_number),
            ))
        }
        _ => Ok((
            ConversationState::ConfirmSchedule,
            retry_actions(
                &context.phone_number,
                "Confirma la programación o elige cambiarla.",
                confirm_schedule_actions(context),
            ),
        )),
    }
}

pub fn when_delivery_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendButtons {
        to: phone.to_string(),
        body: "¿Cuándo lo necesitas?".to_string(),
        buttons: vec![
            reply_button(IMMEDIATE_DELIVERY, "Entrega Inmediata"),
            reply_button(SCHEDULED_DELIVERY, "Entrega Programada"),
        ],
    }]
}

pub fn out_of_hours_actions(phone: &str) -> Vec<BotAction> {
    vec![
        BotAction::SendText {
            to: phone.to_string(),
            body: "Estamos fuera del horario de entrega inmediata. Puedes programar tu pedido para después o intentar contactar asesor.".to_string(),
        },
        BotAction::SendButtons {
            to: phone.to_string(),
            body: "¿Qué deseas hacer?".to_string(),
            buttons: vec![
                reply_button(SCHEDULE_LATER, "Programar"),
                reply_button(CONTACT_ADVISOR_NOW, "Asesor"),
                reply_button(BACK_MAIN_MENU, "Menú"),
            ],
        },
    ]
}

pub fn select_date_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: "Escribe la fecha de entrega. Formatos válidos: 2026-03-15, 15/03/2026 o 15-03-2026.".to_string(),
    }]
}

pub fn select_time_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: "Escribe la hora de entrega. Ejemplos: 15:30, 3:30pm o 3pm.".to_string(),
    }]
}

pub fn confirm_schedule_actions(context: &ConversationContext) -> Vec<BotAction> {
    let date = context
        .scheduled_date
        .as_deref()
        .unwrap_or("fecha pendiente");
    let time = context
        .scheduled_time
        .as_deref()
        .unwrap_or("hora pendiente");

    vec![
        BotAction::SendText {
            to: context.phone_number.clone(),
            body: format!(
                "Entrega programada para el {} a las {}. ¿Confirmas?",
                date, time
            ),
        },
        BotAction::SendButtons {
            to: context.phone_number.clone(),
            body: "Confirma la programación.".to_string(),
            buttons: vec![
                reply_button(CONFIRM_SCHEDULE, "Confirmar"),
                reply_button(CHANGE_SCHEDULE, "Cambiar"),
            ],
        },
    ]
}

pub fn parse_future_date(input: &str, today: NaiveDate) -> Result<NaiveDate, String> {
    let trimmed = input.trim();
    let formats = ["%Y-%m-%d", "%d/%m/%Y", "%d-%m-%Y"];

    let parsed = formats
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(trimmed, format).ok())
        .ok_or_else(|| "La fecha no tiene un formato válido.".to_string())?;

    if parsed <= today {
        return Err("La fecha debe ser futura.".to_string());
    }

    Ok(parsed)
}

pub fn parse_time(input: &str) -> Result<NaiveTime, String> {
    let raw = input.trim().to_lowercase().replace(' ', "");

    if let Ok(time) = NaiveTime::parse_from_str(&raw, "%H:%M") {
        return Ok(time);
    }

    if let Ok(hour) = raw.strip_suffix("am").unwrap_or("").parse::<u32>() {
        if (1..=12).contains(&hour) {
            return Ok(NaiveTime::from_hms_opt(hour % 12, 0, 0).expect("valid am time"));
        }
    }

    if let Ok(hour) = raw.strip_suffix("pm").unwrap_or("").parse::<u32>() {
        if (1..=12).contains(&hour) {
            let hour_24 = if hour == 12 { 12 } else { hour + 12 };
            return Ok(NaiveTime::from_hms_opt(hour_24, 0, 0).expect("valid pm time"));
        }
    }

    if let Some(value) = raw.strip_suffix("am") {
        if let Some(time) = parse_meridian_time(value, false) {
            return Ok(time);
        }
    }

    if let Some(value) = raw.strip_suffix("pm") {
        if let Some(time) = parse_meridian_time(value, true) {
            return Ok(time);
        }
    }

    Err("La hora no tiene un formato válido.".to_string())
}

pub fn is_within_business_hours(time: NaiveTime) -> bool {
    let start = NaiveTime::from_hms_opt(8, 0, 0).expect("static time");
    let end = NaiveTime::from_hms_opt(23, 0, 0).expect("static time");
    time >= start && time <= end
}

fn parse_meridian_time(value: &str, is_pm: bool) -> Option<NaiveTime> {
    let mut parts = value.split(':');
    let hour = parts.next()?.parse::<u32>().ok()?;
    let minute = parts.next()?.parse::<u32>().ok()?;

    if parts.next().is_some() || !(1..=12).contains(&hour) || minute > 59 {
        return None;
    }

    let hour_24 = match (is_pm, hour) {
        (false, 12) => 0,
        (false, h) => h,
        (true, 12) => 12,
        (true, h) => h + 12,
    };

    NaiveTime::from_hms_opt(hour_24, minute, 0)
}

fn now_bogota() -> chrono::DateTime<FixedOffset> {
    let offset = FixedOffset::west_opt(5 * 3600).expect("valid offset");
    Utc::now().with_timezone(&offset)
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
        UserInput::TextMessage(text) if text.trim().eq_ignore_ascii_case("menu") => {
            Some(BACK_MAIN_MENU.to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveTime};

    use crate::bot::state_machine::{ConversationContext, ConversationState, UserInput};

    use super::{
        handle_check_schedule, handle_confirm_schedule, handle_out_of_hours, handle_select_date,
        handle_select_time, handle_when_delivery, is_within_business_hours, parse_future_date,
        parse_time,
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
            pending_has_liquor: None,
            pending_flavor: None,
        }
    }

    #[test]
    fn validates_future_date() {
        let parsed = parse_future_date("2026-03-15", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap())
            .expect("future date");

        assert_eq!(parsed, NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
    }

    #[test]
    fn rejects_past_date() {
        let err = parse_future_date("2026-03-09", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap())
            .expect_err("past date");

        assert_eq!(err, "La fecha debe ser futura.");
    }

    #[test]
    fn parses_time_formats() {
        assert_eq!(parse_time("15:30").unwrap(), NaiveTime::from_hms_opt(15, 30, 0).unwrap());
        assert_eq!(parse_time("3pm").unwrap(), NaiveTime::from_hms_opt(15, 0, 0).unwrap());
        assert_eq!(
            parse_time("3:45pm").unwrap(),
            NaiveTime::from_hms_opt(15, 45, 0).unwrap()
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

    #[test]
    fn when_delivery_scheduled_moves_to_select_date() {
        let mut context = context();
        let (state, _) = handle_when_delivery(
            &UserInput::ButtonPress("scheduled_delivery".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectDate);
        assert_eq!(context.delivery_type.as_deref(), Some("scheduled"));
    }

    #[test]
    fn out_of_hours_can_navigate_to_contact_advisor() {
        let mut context = context();
        let (state, _) = handle_out_of_hours(
            &UserInput::ButtonPress("contact_advisor_now".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::ContactAdvisorName);
    }

    #[test]
    fn select_date_accepts_valid_text() {
        let mut context = context();
        let (state, _) =
            handle_select_date(&UserInput::TextMessage("2026-03-20".to_string()), &mut context)
                .expect("transition");

        assert_eq!(state, ConversationState::SelectTime);
        assert_eq!(context.scheduled_date.as_deref(), Some("2026-03-20"));
    }

    #[test]
    fn select_time_accepts_valid_text() {
        let mut context = context();
        context.scheduled_date = Some("2026-03-20".to_string());
        let (state, _) =
            handle_select_time(&UserInput::TextMessage("4pm".to_string()), &mut context)
                .expect("transition");

        assert_eq!(state, ConversationState::ConfirmSchedule);
        assert_eq!(context.scheduled_time.as_deref(), Some("16:00"));
    }

    #[test]
    fn confirm_schedule_changes_back_to_date() {
        let mut context = context();
        let (state, _) = handle_confirm_schedule(
            &UserInput::ButtonPress("change_schedule".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectDate);
    }

    #[test]
    fn check_schedule_routes_somewhere_valid() {
        let mut context = context();
        let (state, _) = handle_check_schedule(&mut context).expect("transition");

        assert!(matches!(
            state,
            ConversationState::CollectName | ConversationState::OutOfHours
        ));
    }
}
