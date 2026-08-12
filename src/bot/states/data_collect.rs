use crate::bot::state_machine::BotAction;
use crate::messages::client_messages;

pub fn collect_name_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages().data_collect.ask_name.clone(),
    }]
}

pub fn collect_phone_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages().data_collect.ask_phone.clone(),
    }]
}

pub fn collect_address_actions(phone: &str) -> Vec<BotAction> {
    vec![BotAction::SendText {
        to: phone.to_string(),
        body: client_messages().data_collect.ask_address.clone(),
    }]
}

pub fn validate_name(input: &str) -> Result<String, String> {
    let normalized = collapse_spaces(input);
    let length = normalized.chars().count();
    if !(2..=80).contains(&length) {
        return Err(client_messages().data_collect.name_length_error.clone());
    }

    Ok(normalized)
}

pub fn validate_phone(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if !trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(client_messages().data_collect.phone_digits_error.clone());
    }
    if !(7..=15).contains(&trimmed.len()) {
        return Err(client_messages().data_collect.phone_length_error.clone());
    }

    Ok(trimmed.to_string())
}

pub fn validate_address(input: &str) -> Result<String, String> {
    let normalized = collapse_spaces(input);
    let length = normalized.chars().count();
    if !(5..=160).contains(&length) {
        return Err(client_messages().data_collect.address_length_error.clone());
    }

    Ok(normalized)
}

fn collapse_spaces(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{validate_address, validate_name, validate_phone};

    #[test]
    fn validates_name() {
        assert_eq!(validate_name("  Ana   Maria ").unwrap(), "Ana Maria");
    }

    #[test]
    fn validates_phone() {
        assert_eq!(validate_phone("3001234567").unwrap(), "3001234567");
    }

    #[test]
    fn validates_address() {
        assert_eq!(
            validate_address(" Cra 15   #20-30 Armenia ").unwrap(),
            "Cra 15 #20-30 Armenia"
        );
    }
}
