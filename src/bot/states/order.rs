use crate::{
    bot::state_machine::BotAction,
    messages::client_messages,
    whatsapp::types::{Button, ButtonReplyPayload},
};

const WITH_LIQUOR: &str = "with_liquor";
const WITHOUT_LIQUOR: &str = "without_liquor";

pub fn select_type_actions(phone: &str) -> Vec<BotAction> {
    let messages = &client_messages().order;
    vec![BotAction::SendButtons {
        to: phone.to_string(),
        body: messages.select_type_body.clone(),
        buttons: vec![
            reply_button(WITH_LIQUOR, &messages.with_liquor_button),
            reply_button(WITHOUT_LIQUOR, &messages.without_liquor_button),
        ],
    }]
}

pub fn validate_quantity(input: &str) -> Result<u32, String> {
    let quantity = input
        .trim()
        .parse::<u32>()
        .map_err(|_| client_messages().order.quantity_parse_error.clone())?;

    if !(1..=999).contains(&quantity) {
        return Err(client_messages().order.quantity_range_error.clone());
    }

    Ok(quantity)
}

/// Resuelve el nombre de un sabor.
///
/// Desde 2026-09-22 la fuente es el catálogo vivo de `crm-app`
/// (`bot::flavors`), no `config/messages.toml`. Solo resuelve sabores ACTIVOS:
/// un sabor apagado desde el panel deja de poder agregarse a un pedido, que es
/// justo el punto de poder apagarlo.
///
/// El TOML queda de último recurso para los sabores que la tabla no conozca.
pub fn flavor_by_id(id: &str, has_liquor: bool) -> Option<String> {
    let table = crate::bot::flavors::current_flavor_table();
    if let Some(entry) = table.find(id, has_liquor) {
        return entry.active.then(|| entry.name.clone());
    }

    let messages = &client_messages().order;
    if has_liquor {
        messages.flavors_with_liquor.get(id)
    } else {
        messages.flavors_without_liquor.get(id)
    }
    .cloned()
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
    use super::validate_quantity;

    #[test]
    fn validates_quantity() {
        assert_eq!(validate_quantity("12").unwrap(), 12);
    }

    #[test]
    fn flavor_titles_fit_whatsapp_list_limit() {
        let messages = crate::messages::client_messages();

        for title in messages
            .order
            .flavors_with_liquor
            .values()
            .chain(messages.order.flavors_without_liquor.values())
        {
            assert!(
                title.chars().count() <= 24,
                "flavor title exceeds Meta list limit: {title}"
            );
        }
    }
}
