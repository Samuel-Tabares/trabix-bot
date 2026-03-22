use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::Path,
    sync::OnceLock,
};

use serde::Deserialize;

pub const DEFAULT_MESSAGES_PATH: &str = "config/messages.toml";

static CLIENT_MESSAGES: OnceLock<ClientMessages> = OnceLock::new();

const REQUIRED_LIQUOR_FLAVOR_IDS: [&str; 7] = [
    "liquor_maracumango_ron_blanco",
    "liquor_blueberry_vodka",
    "liquor_uva_vodka",
    "liquor_bonbonbum_whiskey",
    "liquor_bonbonbum_fresa_champagne",
    "liquor_smirnoff_lulo",
    "liquor_manzana_verde_tequila",
];

const REQUIRED_NON_LIQUOR_FLAVOR_IDS: [&str; 4] = [
    "non_liquor_maracumango",
    "non_liquor_manzana_verde",
    "non_liquor_bonbonbum",
    "non_liquor_blueberry",
];

#[derive(Debug)]
pub enum MessagesError {
    Io {
        path: String,
        source: std::io::Error,
    },
    Parse(toml::de::Error),
    Validation(String),
    AlreadyInitialized,
}

impl fmt::Display for MessagesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read messages file {path}: {source}")
            }
            Self::Parse(source) => write!(f, "failed to parse client messages: {source}"),
            Self::Validation(message) => write!(f, "invalid client messages: {message}"),
            Self::AlreadyInitialized => write!(f, "client messages were already initialized"),
        }
    }
}

impl Error for MessagesError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse(source) => Some(source),
            Self::Validation(_) | Self::AlreadyInitialized => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientMessages {
    pub menu: MenuMessages,
    pub scheduling: SchedulingMessages,
    pub data_collect: DataCollectMessages,
    pub order: OrderMessages,
    pub checkout: CheckoutMessages,
    pub advisor_customer: AdvisorCustomerMessages,
    pub relay_customer: RelayCustomerMessages,
    pub timers_customer: TimerCustomerMessages,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MenuMessages {
    pub main_welcome: String,
    pub main_list_body: String,
    pub main_list_button_text: String,
    pub main_section_title: String,
    pub make_order_title: String,
    pub make_order_description: String,
    pub view_menu_title: String,
    pub view_menu_description: String,
    pub view_schedule_title: String,
    pub view_schedule_description: String,
    pub contact_advisor_title: String,
    pub contact_advisor_description: String,
    pub retry_main_menu: String,
    pub menu_image_caption: String,
    pub menu_text: String,
    pub view_menu_buttons_body: String,
    pub view_menu_make_order_button: String,
    pub view_menu_back_button: String,
    pub retry_view_menu: String,
    pub schedule_text: String,
    pub schedule_buttons_body: String,
    pub schedule_make_order_button: String,
    pub schedule_back_button: String,
    pub retry_view_schedule: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulingMessages {
    pub when_delivery_body: String,
    pub immediate_button: String,
    pub scheduled_button: String,
    pub retry_when_delivery: String,
    pub out_of_hours_text: String,
    pub out_of_hours_buttons_body: String,
    pub out_of_hours_schedule_button: String,
    pub out_of_hours_advisor_button: String,
    pub out_of_hours_menu_button: String,
    pub out_of_hours_retry: String,
    pub select_date_prompt: String,
    pub select_date_retry_non_text: String,
    pub date_length_error: String,
    pub select_time_prompt: String,
    pub select_time_retry_non_text: String,
    pub time_length_error: String,
    pub confirm_template: String,
    pub confirm_buttons_body: String,
    pub confirm_button: String,
    pub change_button: String,
    pub confirm_retry: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataCollectMessages {
    pub ask_name: String,
    pub ask_phone: String,
    pub ask_address: String,
    pub retry_name_non_text: String,
    pub retry_phone_non_text: String,
    pub retry_address_non_text: String,
    pub name_length_error: String,
    pub phone_digits_error: String,
    pub phone_length_error: String,
    pub address_length_error: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderMessages {
    pub select_type_body: String,
    pub with_liquor_button: String,
    pub without_liquor_button: String,
    pub retry_select_type: String,
    pub select_flavor_with_liquor_body: String,
    pub select_flavor_without_liquor_body: String,
    pub flavor_button_text: String,
    pub flavor_section_title: String,
    pub flavor_row_description: String,
    pub retry_select_flavor: String,
    pub flavors_with_liquor: BTreeMap<String, String>,
    pub flavors_without_liquor: BTreeMap<String, String>,
    pub quantity_prompt_template: String,
    pub quantity_kind_with_liquor: String,
    pub quantity_kind_without_liquor: String,
    pub retry_quantity_non_text: String,
    pub quantity_parse_error: String,
    pub quantity_range_error: String,
    pub added_to_order_template: String,
    pub partial_summary_line_template: String,
    pub partial_kind_with_liquor: String,
    pub partial_kind_without_liquor: String,
    pub add_more_body: String,
    pub add_more_button: String,
    pub finish_order_button: String,
    pub retry_add_more: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CheckoutMessages {
    pub modify_order_text: String,
    pub receipt_image_required: String,
    pub change_address_prompt: String,
    pub change_address_non_text: String,
    pub summary_template: String,
    pub summary_delivery_immediate: String,
    pub summary_delivery_scheduled_template: String,
    pub summary_delivery_pending: String,
    pub summary_items_empty: String,
    pub summary_item_line_template: String,
    pub summary_item_promo_template: String,
    pub summary_item_kind_with_liquor: String,
    pub summary_item_kind_without_liquor: String,
    pub summary_item_mode_wholesale: String,
    pub summary_item_mode_detail: String,
    pub summary_list_body: String,
    pub summary_list_button_text: String,
    pub summary_section_title: String,
    pub cash_on_delivery_title: String,
    pub cash_on_delivery_description: String,
    pub pay_now_title: String,
    pub pay_now_description: String,
    pub modify_order_title: String,
    pub modify_order_description: String,
    pub cancel_order_title: String,
    pub cancel_order_description: String,
    pub confirm_address_template: String,
    pub confirm_address_button: String,
    pub change_address_button: String,
    pub transfer_payment_text: String,
    pub wait_receipt_prompt: String,
    pub receipt_timeout_body: String,
    pub receipt_timeout_change_payment_button: String,
    pub receipt_timeout_cancel_button: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdvisorCustomerMessages {
    pub contact_name_prompt: String,
    pub contact_phone_prompt: String,
    pub contact_name_retry_non_text: String,
    pub contact_phone_retry_non_text: String,
    pub wait_contact_initial_text: String,
    pub wait_contact_repeat_text: String,
    pub wait_contact_leave_message_prompt: String,
    pub leave_message_non_text: String,
    pub leave_message_length_error: String,
    pub leave_message_success: String,
    pub wait_delivery_cost_text: String,
    pub wait_negotiate_hour_text: String,
    pub wait_advisor_hour_decision_text: String,
    pub wait_advisor_confirm_text: String,
    pub wait_general_text: String,
    pub availability_wait_text: String,
    pub wholesale_wait_text: String,
    pub order_sent_text: String,
    pub scheduled_order_sent_text: String,
    pub wholesale_order_sent_text: String,
    pub proposed_hour_question_template: String,
    pub proposed_hour_buttons_body: String,
    pub accept_button: String,
    pub reject_button: String,
    pub proposed_hour_repeat_template: String,
    pub client_hour_prompt: String,
    pub client_hour_retry_non_text: String,
    pub hour_length_error: String,
    pub confirmed_order_template: String,
    pub scheduled_confirmation_template: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelayCustomerMessages {
    pub direct_contact_connected_text: String,
    pub wholesale_connected_text: String,
    pub relay_text_only: String,
    pub relay_closed_by_timeout: String,
    pub relay_closed_manual: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TimerCustomerMessages {
    pub receipt_timeout_text: String,
    pub receipt_timeout_buttons_body: String,
    pub advisor_timeout_text: String,
    pub advisor_timeout_wholesale_text: String,
    pub advisor_timeout_buttons_body: String,
    pub advisor_timeout_schedule_button: String,
    pub advisor_timeout_retry_button: String,
    pub advisor_timeout_menu_button: String,
    pub contact_timeout_body: String,
    pub contact_timeout_leave_message_button: String,
    pub contact_timeout_menu_button: String,
    pub relay_timeout_text: String,
}

impl ClientMessages {
    pub fn load_default() -> Result<Self, MessagesError> {
        Self::from_path(DEFAULT_MESSAGES_PATH)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, MessagesError> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path).map_err(|source| MessagesError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_toml_str(&raw)
    }

    fn from_toml_str(raw: &str) -> Result<Self, MessagesError> {
        let messages: Self = toml::from_str(raw).map_err(MessagesError::Parse)?;
        messages.validate()?;
        Ok(messages)
    }

    fn validate(&self) -> Result<(), MessagesError> {
        validate_template(
            &self.scheduling.confirm_template,
            &["date", "time"],
            "scheduling.confirm_template",
        )?;
        validate_template(
            &self.order.quantity_prompt_template,
            &["flavor", "kind"],
            "order.quantity_prompt_template",
        )?;
        validate_template(
            &self.order.added_to_order_template,
            &["summary"],
            "order.added_to_order_template",
        )?;
        validate_template(
            &self.order.partial_summary_line_template,
            &["quantity", "flavor", "kind"],
            "order.partial_summary_line_template",
        )?;
        validate_template(
            &self.checkout.summary_template,
            &[
                "customer_name",
                "customer_phone",
                "delivery_address",
                "delivery",
                "items",
                "total_estimated",
            ],
            "checkout.summary_template",
        )?;
        validate_template(
            &self.checkout.summary_delivery_scheduled_template,
            &["date", "time"],
            "checkout.summary_delivery_scheduled_template",
        )?;
        validate_template(
            &self.checkout.summary_item_line_template,
            &["quantity", "flavor", "kind", "mode", "subtotal", "promo"],
            "checkout.summary_item_line_template",
        )?;
        validate_template(
            &self.checkout.summary_item_promo_template,
            &["promo_units"],
            "checkout.summary_item_promo_template",
        )?;
        validate_template(
            &self.checkout.confirm_address_template,
            &["address"],
            "checkout.confirm_address_template",
        )?;
        validate_template(
            &self.advisor_customer.proposed_hour_question_template,
            &["hour"],
            "advisor_customer.proposed_hour_question_template",
        )?;
        validate_template(
            &self.advisor_customer.proposed_hour_repeat_template,
            &["hour"],
            "advisor_customer.proposed_hour_repeat_template",
        )?;
        validate_template(
            &self.advisor_customer.confirmed_order_template,
            &["subtotal", "delivery_cost", "total_final", "address"],
            "advisor_customer.confirmed_order_template",
        )?;
        validate_template(
            &self.advisor_customer.scheduled_confirmation_template,
            &["date", "time"],
            "advisor_customer.scheduled_confirmation_template",
        )?;
        validate_keys(
            &self.order.flavors_with_liquor,
            &REQUIRED_LIQUOR_FLAVOR_IDS,
            "order.flavors_with_liquor",
        )?;
        validate_keys(
            &self.order.flavors_without_liquor,
            &REQUIRED_NON_LIQUOR_FLAVOR_IDS,
            "order.flavors_without_liquor",
        )?;

        Ok(())
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self::from_toml_str(include_str!("../config/messages.toml"))
            .expect("config/messages.toml should stay valid")
    }
}

pub fn set_client_messages(messages: ClientMessages) -> Result<(), MessagesError> {
    CLIENT_MESSAGES
        .set(messages)
        .map_err(|_| MessagesError::AlreadyInitialized)
}

pub fn client_messages() -> &'static ClientMessages {
    #[cfg(test)]
    {
        return CLIENT_MESSAGES.get_or_init(ClientMessages::for_tests);
    }

    #[cfg(not(test))]
    {
        CLIENT_MESSAGES
            .get()
            .expect("client messages must be initialized before use")
    }
}

pub fn render_template(template: &str, values: &[(&str, &str)]) -> String {
    let mut rendered = template.to_string();
    for (key, value) in values {
        rendered = rendered.replace(&format!("{{{key}}}"), value);
    }
    rendered
}

fn validate_template(template: &str, expected: &[&str], field: &str) -> Result<(), MessagesError> {
    let actual = extract_placeholders(template)
        .map_err(|message| MessagesError::Validation(format!("{field}: {message}")))?;
    let expected = expected
        .iter()
        .map(|value| (*value).to_string())
        .collect::<BTreeSet<_>>();

    if actual != expected {
        return Err(MessagesError::Validation(format!(
            "{field}: expected placeholders {:?}, found {:?}",
            expected, actual
        )));
    }

    Ok(())
}

fn validate_keys(
    values: &BTreeMap<String, String>,
    expected: &[&str],
    field: &str,
) -> Result<(), MessagesError> {
    let actual = values.keys().cloned().collect::<BTreeSet<_>>();
    let expected = expected
        .iter()
        .map(|value| (*value).to_string())
        .collect::<BTreeSet<_>>();

    if actual != expected {
        return Err(MessagesError::Validation(format!(
            "{field}: expected keys {:?}, found {:?}",
            expected, actual
        )));
    }

    Ok(())
}

fn extract_placeholders(template: &str) -> Result<BTreeSet<String>, String> {
    let mut placeholders = BTreeSet::new();
    let chars = template.chars().collect::<Vec<_>>();
    let mut index = 0;

    while index < chars.len() {
        match chars[index] {
            '{' => {
                let start = index + 1;
                let mut end = start;
                while end < chars.len() && chars[end] != '}' {
                    if chars[end] == '{' {
                        return Err("nested '{' found inside placeholder".to_string());
                    }
                    end += 1;
                }
                if end == chars.len() {
                    return Err("unclosed '{' in template".to_string());
                }

                let name = chars[start..end].iter().collect::<String>();
                if name.is_empty() {
                    return Err("empty placeholder is not allowed".to_string());
                }
                if !name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                {
                    return Err(format!("invalid placeholder name '{name}'"));
                }
                placeholders.insert(name);
                index = end + 1;
            }
            '}' => return Err("closing '}' without matching '{'".to_string()),
            _ => index += 1,
        }
    }

    Ok(placeholders)
}

#[cfg(test)]
mod tests {
    use super::ClientMessages;

    #[test]
    fn loads_messages_from_repo_fixture() {
        let messages = ClientMessages::for_tests();

        assert_eq!(messages.menu.main_list_button_text, "Ver opciones");
        assert_eq!(
            messages.checkout.receipt_timeout_change_payment_button,
            "Cambiar pago"
        );
    }

    #[test]
    fn rejects_invalid_placeholders() {
        let broken = include_str!("../config/messages.toml").replace(
            "confirm_template = \"\"\"📦 Entrega programada\nFecha: {date}\nHora de referencia: {time}\n\n¿Así está bien?\"\"\"",
            "confirm_template = \"📦 Entrega programada\"",
        );

        let error = ClientMessages::from_toml_str(&broken).expect_err("should fail");

        assert!(
            error.to_string().contains("scheduling.confirm_template"),
            "unexpected error: {error}"
        );
    }
}
