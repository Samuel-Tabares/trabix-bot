use std::env;

use chrono::{
    DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
    Weekday,
};

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
        body: "Escribe la fecha de entrega como prefieras. Ejemplos: mañana, 24/12, 24 de diciembre o viernes.".to_string(),
    }]
}

pub fn select_time_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: "Escribe la hora de entrega como prefieras. Ejemplos: 3pm, 3 y media, 15:30 o a las 4 de la tarde.".to_string(),
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
    let normalized = normalize_for_matching(input);

    let parsed = parse_relative_date(&normalized, today)
        .or_else(|| parse_absolute_date(input, &normalized, today))
        .or_else(|| parse_weekday_date(&normalized, today))
        .ok_or_else(|| {
            "No pude entender la fecha. Prueba con algo como mañana, 24/12 o viernes.".to_string()
        })?;

    if parsed < today {
        return Err("La fecha debe ser hoy o futura.".to_string());
    }

    Ok(parsed)
}

pub fn parse_time(input: &str) -> Result<NaiveTime, String> {
    let normalized = normalize_for_matching(input);
    let compact = normalized.replace(' ', "");

    if normalized == "mediodia" {
        return Ok(NaiveTime::from_hms_opt(12, 0, 0).expect("static time"));
    }

    if normalized == "medianoche" {
        return Ok(NaiveTime::from_hms_opt(0, 0, 0).expect("static time"));
    }

    if let Ok(time) = NaiveTime::parse_from_str(&compact, "%H:%M") {
        return Ok(time);
    }

    if let Ok(hour) = compact.parse::<u32>() {
        return build_time_from_hour_only(hour, detect_period_hint(&normalized));
    }

    if let Some(time) = parse_textual_time(&normalized) {
        return Ok(time);
    }

    if let Some(time) = parse_meridian_compact_time(&compact) {
        return Ok(time);
    }

    Err("No pude entender la hora. Prueba con algo como 3pm, 3 y media o 15:30.".to_string())
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
    if let Some(forced) = parse_forced_bogota_now(offset) {
        return forced;
    }

    Utc::now().with_timezone(&offset)
}

fn parse_relative_date(normalized: &str, today: NaiveDate) -> Option<NaiveDate> {
    match normalized {
        "hoy" => Some(today),
        "manana" => Some(today + Duration::days(1)),
        "pasado manana" => Some(today + Duration::days(2)),
        _ => None,
    }
}

fn parse_absolute_date(raw: &str, normalized: &str, today: NaiveDate) -> Option<NaiveDate> {
    let trimmed = raw.trim();
    let formats = ["%Y-%m-%d", "%d/%m/%Y", "%d-%m-%Y"];

    if let Some(date) = formats
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(trimmed, format).ok())
    {
        return Some(date);
    }

    if let Some(date) = parse_day_month_without_year(normalized, today) {
        return Some(date);
    }

    parse_day_month_with_words(normalized, today)
}

fn parse_day_month_without_year(normalized: &str, today: NaiveDate) -> Option<NaiveDate> {
    for separator in ['/', '-'] {
        let mut parts = normalized.split(separator);
        let day = parts.next()?.parse::<u32>().ok()?;
        let month = parts.next()?.parse::<u32>().ok()?;

        if let Some(year) = parts.next() {
            if !year.is_empty() {
                return None;
            }
        }

        let candidate = NaiveDate::from_ymd_opt(today.year(), month, day)?;
        return if candidate < today {
            NaiveDate::from_ymd_opt(today.year() + 1, month, day)
        } else {
            Some(candidate)
        };
    }

    None
}

fn parse_day_month_with_words(normalized: &str, today: NaiveDate) -> Option<NaiveDate> {
    let tokens: Vec<&str> = normalized
        .split_whitespace()
        .filter(|token| *token != "de")
        .collect();

    if tokens.len() < 2 {
        return None;
    }

    let day = tokens.first()?.parse::<u32>().ok()?;
    let month = month_number(tokens.get(1)?)?;
    let year = tokens
        .get(2)
        .and_then(|token| token.parse::<i32>().ok())
        .unwrap_or(today.year());

    let candidate = NaiveDate::from_ymd_opt(year, month, day)?;
    if tokens.len() >= 3 {
        return Some(candidate);
    }

    if candidate < today {
        NaiveDate::from_ymd_opt(today.year() + 1, month, day)
    } else {
        Some(candidate)
    }
}

fn parse_weekday_date(normalized: &str, today: NaiveDate) -> Option<NaiveDate> {
    let target = weekday_number(normalized)?;
    let today_number = today.weekday().num_days_from_monday() as i64;
    let target_number = target.num_days_from_monday() as i64;
    let mut delta = (target_number - today_number + 7) % 7;
    if delta == 0 {
        delta = 7;
    }
    Some(today + Duration::days(delta))
}

fn parse_textual_time(normalized: &str) -> Option<NaiveTime> {
    let period = detect_period_hint(normalized);
    let cleaned = normalized
        .replace("a las", "")
        .replace("a la", "")
        .replace("de la", "")
        .replace("por la", "")
        .replace("manana", "")
        .replace("tarde", "")
        .replace("noche", "")
        .replace("madrugada", "")
        .replace(" en punto", "")
        .replace(" de ", " ")
        .replace(" pm", "pm")
        .replace(" am", "am");
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");

    if cleaned.contains(" y media") {
        let hour = cleaned
            .split(" y media")
            .next()?
            .trim()
            .parse::<u32>()
            .ok()?;
        return build_time(hour, 30, period);
    }

    if cleaned.contains(" y cuarto") {
        let hour = cleaned
            .split(" y cuarto")
            .next()?
            .trim()
            .parse::<u32>()
            .ok()?;
        return build_time(hour, 15, period);
    }

    if let Some(base) = cleaned
        .strip_suffix("pm")
        .or_else(|| cleaned.strip_suffix("am"))
    {
        let is_pm = cleaned.ends_with("pm");
        if base.contains(':') {
            return parse_meridian_time(base, is_pm);
        }
        let hour = base.trim().parse::<u32>().ok()?;
        return build_time(
            hour,
            0,
            if is_pm {
                Some(PeriodHint::Pm)
            } else {
                Some(PeriodHint::Am)
            },
        );
    }

    if let Some((hour, minute)) = cleaned.split_once(':') {
        let hour = hour.trim().parse::<u32>().ok()?;
        let minute = minute.trim().parse::<u32>().ok()?;
        return build_time(hour, minute, period);
    }

    let hour = cleaned.trim().parse::<u32>().ok()?;
    build_time(hour, 0, period)
}

fn parse_meridian_compact_time(compact: &str) -> Option<NaiveTime> {
    if let Ok(hour) = compact.strip_suffix("am").unwrap_or("").parse::<u32>() {
        return build_time(hour, 0, Some(PeriodHint::Am));
    }

    if let Ok(hour) = compact.strip_suffix("pm").unwrap_or("").parse::<u32>() {
        return build_time(hour, 0, Some(PeriodHint::Pm));
    }

    if let Some(value) = compact.strip_suffix("am") {
        return parse_meridian_time(value, false);
    }

    if let Some(value) = compact.strip_suffix("pm") {
        return parse_meridian_time(value, true);
    }

    None
}

fn build_time_from_hour_only(hour: u32, period: Option<PeriodHint>) -> Result<NaiveTime, String> {
    build_time(hour, 0, period).ok_or_else(|| {
        "No pude entender la hora. Prueba con algo como 3pm, 3 y media o 15:30.".to_string()
    })
}

fn build_time(hour: u32, minute: u32, period: Option<PeriodHint>) -> Option<NaiveTime> {
    if minute > 59 {
        return None;
    }

    if hour > 23 {
        return None;
    }

    if hour > 12 {
        return NaiveTime::from_hms_opt(hour, minute, 0);
    }

    let hour_24 = match period {
        Some(PeriodHint::Am) => {
            if hour == 12 {
                0
            } else {
                hour
            }
        }
        Some(PeriodHint::Pm) => {
            if hour == 12 {
                12
            } else {
                hour + 12
            }
        }
        None => default_meridiem(hour)?,
    };

    NaiveTime::from_hms_opt(hour_24, minute, 0)
}

fn default_meridiem(hour: u32) -> Option<u32> {
    match hour {
        0..=7 => Some(if hour == 0 { 0 } else { hour + 12 }),
        8..=11 => Some(hour),
        12 => Some(12),
        _ => None,
    }
}

fn detect_period_hint(normalized: &str) -> Option<PeriodHint> {
    if normalized.contains("am")
        || normalized.contains("manana")
        || normalized.contains("madrugada")
    {
        return Some(PeriodHint::Am);
    }

    if normalized.contains("pm") || normalized.contains("tarde") || normalized.contains("noche") {
        return Some(PeriodHint::Pm);
    }

    None
}

fn month_number(token: &str) -> Option<u32> {
    match token {
        "enero" => Some(1),
        "febrero" => Some(2),
        "marzo" => Some(3),
        "abril" => Some(4),
        "mayo" => Some(5),
        "junio" => Some(6),
        "julio" => Some(7),
        "agosto" => Some(8),
        "septiembre" | "setiembre" => Some(9),
        "octubre" => Some(10),
        "noviembre" => Some(11),
        "diciembre" => Some(12),
        _ => None,
    }
}

fn weekday_number(token: &str) -> Option<Weekday> {
    match token {
        "lunes" => Some(Weekday::Mon),
        "martes" => Some(Weekday::Tue),
        "miercoles" => Some(Weekday::Wed),
        "jueves" => Some(Weekday::Thu),
        "viernes" => Some(Weekday::Fri),
        "sabado" => Some(Weekday::Sat),
        "domingo" => Some(Weekday::Sun),
        _ => None,
    }
}

fn normalize_for_matching(input: &str) -> String {
    input
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| match ch {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            _ => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeriodHint {
    Am,
    Pm,
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

    use chrono::{NaiveDate, NaiveTime};

    use crate::bot::state_machine::{ConversationContext, ConversationState, UserInput};

    use super::{
        handle_check_schedule, handle_confirm_schedule, handle_out_of_hours, handle_select_date,
        handle_select_time, handle_when_delivery, is_within_business_hours, now_bogota,
        parse_future_date, parse_time,
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

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
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

        assert_eq!(err, "La fecha debe ser hoy o futura.");
    }

    #[test]
    fn parses_time_formats() {
        assert_eq!(
            parse_time("15:30").unwrap(),
            NaiveTime::from_hms_opt(15, 30, 0).unwrap()
        );
        assert_eq!(
            parse_time("3pm").unwrap(),
            NaiveTime::from_hms_opt(15, 0, 0).unwrap()
        );
        assert_eq!(
            parse_time("3:45pm").unwrap(),
            NaiveTime::from_hms_opt(15, 45, 0).unwrap()
        );
        assert_eq!(
            parse_time("3 y media").unwrap(),
            NaiveTime::from_hms_opt(15, 30, 0).unwrap()
        );
        assert_eq!(
            parse_time("a las 4 de la tarde").unwrap(),
            NaiveTime::from_hms_opt(16, 0, 0).unwrap()
        );
        assert_eq!(
            parse_time("10 de la manana").unwrap(),
            NaiveTime::from_hms_opt(10, 0, 0).unwrap()
        );
    }

    #[test]
    fn parses_natural_date_formats() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();

        assert_eq!(
            parse_future_date("manana", today).unwrap(),
            today.succ_opt().unwrap()
        );
        assert_eq!(
            parse_future_date("24/12", today).unwrap(),
            NaiveDate::from_ymd_opt(2026, 12, 24).unwrap()
        );
        assert_eq!(
            parse_future_date("24 de diciembre", today).unwrap(),
            NaiveDate::from_ymd_opt(2026, 12, 24).unwrap()
        );
        assert_eq!(
            parse_future_date("viernes", today).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 13).unwrap()
        );
    }

    #[test]
    fn supports_clock_override_for_tests() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::set_var("FORCE_BOGOTA_NOW", "2026-03-10 23:30");

        let now = now_bogota();

        std::env::remove_var("FORCE_BOGOTA_NOW");

        assert_eq!(
            now.date_naive(),
            NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()
        );
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
    fn select_date_accepts_valid_text() {
        let mut context = context();
        let (state, _) = handle_select_date(
            &UserInput::TextMessage("2026-03-20".to_string()),
            &mut context,
        )
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
