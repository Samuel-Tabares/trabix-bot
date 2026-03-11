use std::env;

use chrono::{DateTime, FixedOffset, NaiveDateTime, NaiveTime, TimeZone, Utc};

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

const DATE_MIN_LEN: usize = 2;
const DATE_MAX_LEN: usize = 40;
const TIME_MIN_LEN: usize = 1;
const TIME_MAX_LEN: usize = 40;

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
        Some(CONTACT_ADVISOR_NOW) => {
            let (state, actions) = advisor::start_contact_advisor(context);
            Ok((state, actions))
        }
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
        UserInput::TextMessage(text) => match validate_schedule_text(
            text,
            DATE_MIN_LEN,
            DATE_MAX_LEN,
            "La fecha debe tener entre 2 y 40 caracteres.",
        ) {
            Ok(date) => {
                context.scheduled_date = Some(date);
                Ok((
                    ConversationState::SelectTime,
                    select_time_actions(&context.phone_number),
                ))
            }
            Err(message) => Ok((
                ConversationState::SelectDate,
                retry_actions(
                    &context.phone_number,
                    &message,
                    select_date_actions(&context.phone_number),
                ),
            )),
        },
        _ => Ok((
            ConversationState::SelectDate,
            retry_actions(
                &context.phone_number,
                "Escribe una fecha para programar tu pedido.",
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
        UserInput::TextMessage(text) => match validate_schedule_text(
            text,
            TIME_MIN_LEN,
            TIME_MAX_LEN,
            "La hora debe tener entre 1 y 40 caracteres.",
        ) {
            Ok(time) => {
                context.scheduled_time = Some(time);
                Ok((
                    ConversationState::ConfirmSchedule,
                    confirm_schedule_actions(context),
                ))
            }
            Err(message) => Ok((
                ConversationState::SelectTime,
                retry_actions(
                    &context.phone_number,
                    &message,
                    select_time_actions(&context.phone_number),
                ),
            )),
        },
        _ => Ok((
            ConversationState::SelectTime,
            retry_actions(
                &context.phone_number,
                "Escribe una hora para programar tu pedido.",
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
        Some(CONFIRM_SCHEDULE) => {
            if context.schedule_resume_target.is_some() {
                let (state, actions) = advisor::resume_after_schedule_confirmation(context);
                return Ok((state, actions));
            }

            Ok((
                ConversationState::CollectName,
                data_collect::collect_name_actions(&context.phone_number),
            ))
        }
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
        body: "Escribe la fecha de entrega como prefieras. Solo necesitamos una referencia para el asesor.".to_string(),
    }]
}

pub fn select_time_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: "Escribe la hora de entrega como prefieras. Solo necesitamos una referencia para el asesor.".to_string(),
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
                "Entrega programada para: {}\nHora de referencia: {}\n\n¿Confirmas?",
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

pub fn is_within_business_hours(time: NaiveTime) -> bool {
    let start = NaiveTime::from_hms_opt(8, 0, 0).expect("static time");
    let end = NaiveTime::from_hms_opt(23, 0, 0).expect("static time");
    time >= start && time <= end
}

fn validate_schedule_text(
    input: &str,
    min_len: usize,
    max_len: usize,
    error_message: &str,
) -> Result<String, String> {
    let normalized = collapse_spaces(input);
    let length = normalized.chars().count();

    if !(min_len..=max_len).contains(&length) {
        return Err(error_message.to_string());
    }

    Ok(normalized)
}

fn collapse_spaces(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn now_bogota() -> DateTime<FixedOffset> {
    let offset = FixedOffset::west_opt(5 * 3600).expect("valid offset");
    if let Some(forced) = parse_forced_bogota_now(offset) {
        return forced;
    }

    Utc::now().with_timezone(&offset)
}

fn parse_forced_bogota_now(offset: FixedOffset) -> Option<DateTime<FixedOffset>> {
    let raw = env::var("FORCE_BOGOTA_NOW").ok()?;

    if let Ok(datetime) = DateTime::parse_from_rfc3339(&raw) {
        return Some(datetime.with_timezone(&offset));
    }

    let formats = ["%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M"];
    formats
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(&raw, format).ok())
        .and_then(|datetime| offset.from_local_datetime(&datetime).single())
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
    use std::sync::{Mutex, OnceLock};

    use chrono::NaiveTime;

    use crate::bot::state_machine::{ConversationContext, ConversationState, UserInput};

    use super::{
        handle_check_schedule, handle_confirm_schedule, handle_out_of_hours, handle_select_date,
        handle_select_time, handle_when_delivery, is_within_business_hours, now_bogota,
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

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn supports_clock_override_for_tests() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::set_var("FORCE_BOGOTA_NOW", "2026-03-10 23:30");

        let now = now_bogota();

        std::env::remove_var("FORCE_BOGOTA_NOW");

        assert_eq!(now.time(), NaiveTime::from_hms_opt(23, 30, 0).unwrap());
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
    fn check_schedule_routes_somewhere_valid() {
        let mut context = context();
        let (state, _) = handle_check_schedule(&mut context).expect("transition");

        assert!(matches!(
            state,
            ConversationState::CollectName | ConversationState::OutOfHours
        ));
    }

    #[test]
    fn select_date_accepts_flexible_text() {
        let mut context = context();
        let (state, _) = handle_select_date(
            &UserInput::TextMessage("mañna despuesito".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectTime);
        assert_eq!(context.scheduled_date.as_deref(), Some("mañna despuesito"));
    }

    #[test]
    fn select_time_accepts_flexible_text() {
        let mut context = context();
        let (state, _) = handle_select_time(
            &UserInput::TextMessage("como a las 3 o 4".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::ConfirmSchedule);
        assert_eq!(context.scheduled_time.as_deref(), Some("como a las 3 o 4"));
    }

    #[test]
    fn confirm_schedule_changes_back_to_date() {
        let mut context = context();
        context.scheduled_date = Some("mañana".to_string());
        context.scheduled_time = Some("3 y media".to_string());

        let (state, _) = handle_confirm_schedule(
            &UserInput::ButtonPress("change_schedule".to_string()),
            &mut context,
        )
        .expect("transition");

        assert_eq!(state, ConversationState::SelectDate);
        assert_eq!(context.scheduled_date, None);
        assert_eq!(context.scheduled_time, None);
    }
}
