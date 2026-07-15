//! Presupuesto diario de llamadas al LLM. Dos límites, ambos por día
//! calendario de Bogotá: uno por teléfono de cliente (anti-abuso: un
//! desconocido no puede quemar saldo escribiendo mil mensajes) y uno global
//! opcional (`AGENT_DAILY_LLM_CALL_LIMIT`, kill-switch de gasto). Vive en
//! memoria: un redeploy reinicia los contadores, lo cual es aceptable porque
//! el límite protege contra abuso sostenido, no contra contabilidad exacta.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{FixedOffset, NaiveDate, Utc};
use tokio::sync::Mutex;

pub const PER_PHONE_DAILY_LIMIT: u32 = 30;

pub type LlmBudgetHandle = Arc<Mutex<LlmBudget>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetCheck {
    Allowed,
    DeniedFirstNotice,
    Denied,
}

#[derive(Debug)]
pub struct LlmBudget {
    per_phone_limit: u32,
    global_limit: Option<u64>,
    date: NaiveDate,
    global_count: u64,
    per_phone: HashMap<String, u32>,
    denied_notified: HashSet<String>,
}

pub fn new_llm_budget_handle(global_limit: Option<u64>) -> LlmBudgetHandle {
    Arc::new(Mutex::new(LlmBudget::new(PER_PHONE_DAILY_LIMIT, global_limit)))
}

pub fn bogota_today() -> NaiveDate {
    let bogota = FixedOffset::west_opt(5 * 3600).expect("UTC-5 is a valid offset");
    Utc::now().with_timezone(&bogota).date_naive()
}

impl LlmBudget {
    pub fn new(per_phone_limit: u32, global_limit: Option<u64>) -> Self {
        Self {
            per_phone_limit,
            global_limit,
            date: bogota_today(),
            global_count: 0,
            per_phone: HashMap::new(),
            denied_notified: HashSet::new(),
        }
    }

    fn roll_day(&mut self, today: NaiveDate) {
        if self.date != today {
            self.date = today;
            self.global_count = 0;
            self.per_phone.clear();
            self.denied_notified.clear();
        }
    }

    /// Se evalúa una vez al inicio del turno, antes de la primera llamada al
    /// LLM. Un turno ya iniciado puede exceder el límite por hasta
    /// `MAX_TOOL_ITERATIONS - 1` llamadas; ese margen es intencional para no
    /// cortar un turno a mitad de tool-calls.
    pub fn check_turn_start(&mut self, phone: &str, today: NaiveDate) -> BudgetCheck {
        self.roll_day(today);

        let phone_exhausted =
            self.per_phone.get(phone).copied().unwrap_or(0) >= self.per_phone_limit;
        let global_exhausted = self
            .global_limit
            .is_some_and(|limit| self.global_count >= limit);

        if !phone_exhausted && !global_exhausted {
            return BudgetCheck::Allowed;
        }

        if self.denied_notified.insert(phone.to_string()) {
            BudgetCheck::DeniedFirstNotice
        } else {
            BudgetCheck::Denied
        }
    }

    pub fn consume_call(&mut self, phone: &str, today: NaiveDate) {
        self.roll_day(today);
        self.global_count += 1;
        *self.per_phone.entry(phone.to_string()).or_insert(0) += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(n: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, n).expect("valid date")
    }

    #[test]
    fn allows_until_per_phone_limit_then_denies_with_single_notice() {
        let mut budget = LlmBudget::new(3, None);
        for _ in 0..3 {
            assert_eq!(budget.check_turn_start("p1", day(1)), BudgetCheck::Allowed);
            budget.consume_call("p1", day(1));
        }
        assert_eq!(
            budget.check_turn_start("p1", day(1)),
            BudgetCheck::DeniedFirstNotice
        );
        assert_eq!(budget.check_turn_start("p1", day(1)), BudgetCheck::Denied);
        assert_eq!(budget.check_turn_start("p2", day(1)), BudgetCheck::Allowed);
    }

    #[test]
    fn global_limit_blocks_every_phone() {
        let mut budget = LlmBudget::new(100, Some(2));
        budget.consume_call("p1", day(1));
        budget.consume_call("p2", day(1));
        assert_eq!(
            budget.check_turn_start("p3", day(1)),
            BudgetCheck::DeniedFirstNotice
        );
        assert_eq!(budget.check_turn_start("p3", day(1)), BudgetCheck::Denied);
    }

    #[test]
    fn counters_reset_on_new_day() {
        let mut budget = LlmBudget::new(1, Some(1));
        budget.consume_call("p1", day(1));
        assert_eq!(
            budget.check_turn_start("p1", day(1)),
            BudgetCheck::DeniedFirstNotice
        );
        assert_eq!(budget.check_turn_start("p1", day(2)), BudgetCheck::Allowed);
    }
}
