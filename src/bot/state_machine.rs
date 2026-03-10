use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    db::models::{ConversationStateData, OrderItemData},
    whatsapp::types::{Button, IncomingMessage, ListSection},
};

use super::states::{
    advisor, checkout, data_collect, menu, order, relay, scheduling,
};

pub type TransitionResult = Result<(ConversationState, Vec<BotAction>), StateMachineError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationState {
    MainMenu,
    ViewMenu,
    ViewSchedule,
    WhenDelivery,
    CheckSchedule,
    OutOfHours,
    SelectDate,
    SelectTime,
    ConfirmSchedule,
    CollectName,
    CollectPhone,
    CollectAddress,
    SelectType,
    SelectFlavor { has_liquor: bool },
    SelectQuantity { has_liquor: bool, flavor: String },
    AddMore,
    ConfirmAddress,
    ShowSummary,
    WaitReceipt,
    WaitAdvisorResponse,
    AskDeliveryCost,
    NegotiateHour,
    OfferHourToClient { proposed_hour: String },
    WaitClientHour,
    WaitAdvisorHourDecision { client_hour: String },
    WaitAdvisorConfirmHour,
    WaitAdvisorMayor,
    RelayMode,
    ContactAdvisorName,
    ContactAdvisorPhone,
    WaitAdvisorContact,
    LeaveMessage,
    OrderComplete,
}

impl ConversationState {
    pub fn as_storage_key(&self) -> &'static str {
        match self {
            Self::MainMenu => "main_menu",
            Self::ViewMenu => "view_menu",
            Self::ViewSchedule => "view_schedule",
            Self::WhenDelivery => "when_delivery",
            Self::CheckSchedule => "check_schedule",
            Self::OutOfHours => "out_of_hours",
            Self::SelectDate => "select_date",
            Self::SelectTime => "select_time",
            Self::ConfirmSchedule => "confirm_schedule",
            Self::CollectName => "collect_name",
            Self::CollectPhone => "collect_phone",
            Self::CollectAddress => "collect_address",
            Self::SelectType => "select_type",
            Self::SelectFlavor { .. } => "select_flavor",
            Self::SelectQuantity { .. } => "select_quantity",
            Self::AddMore => "add_more",
            Self::ConfirmAddress => "confirm_address",
            Self::ShowSummary => "show_summary",
            Self::WaitReceipt => "wait_receipt",
            Self::WaitAdvisorResponse => "wait_advisor_response",
            Self::AskDeliveryCost => "ask_delivery_cost",
            Self::NegotiateHour => "negotiate_hour",
            Self::OfferHourToClient { .. } => "offer_hour_to_client",
            Self::WaitClientHour => "wait_client_hour",
            Self::WaitAdvisorHourDecision { .. } => "wait_advisor_hour_decision",
            Self::WaitAdvisorConfirmHour => "wait_advisor_confirm_hour",
            Self::WaitAdvisorMayor => "wait_advisor_mayor",
            Self::RelayMode => "relay_mode",
            Self::ContactAdvisorName => "contact_advisor_name",
            Self::ContactAdvisorPhone => "contact_advisor_phone",
            Self::WaitAdvisorContact => "wait_advisor_contact",
            Self::LeaveMessage => "leave_message",
            Self::OrderComplete => "order_complete",
        }
    }

    pub fn from_storage_key(
        key: &str,
        context: &ConversationContext,
    ) -> Result<Self, StateMachineError> {
        match key {
            "main_menu" => Ok(Self::MainMenu),
            "view_menu" => Ok(Self::ViewMenu),
            "view_schedule" => Ok(Self::ViewSchedule),
            "when_delivery" => Ok(Self::WhenDelivery),
            "check_schedule" => Ok(Self::CheckSchedule),
            "out_of_hours" => Ok(Self::OutOfHours),
            "select_date" => Ok(Self::SelectDate),
            "select_time" => Ok(Self::SelectTime),
            "confirm_schedule" => Ok(Self::ConfirmSchedule),
            "collect_name" => Ok(Self::CollectName),
            "collect_phone" => Ok(Self::CollectPhone),
            "collect_address" => Ok(Self::CollectAddress),
            "select_type" => Ok(Self::SelectType),
            "select_flavor" => Ok(Self::SelectFlavor {
                has_liquor: context
                    .pending_has_liquor
                    .ok_or(StateMachineError::MissingContext("pending_has_liquor"))?,
            }),
            "select_quantity" => Ok(Self::SelectQuantity {
                has_liquor: context
                    .pending_has_liquor
                    .ok_or(StateMachineError::MissingContext("pending_has_liquor"))?,
                flavor: context
                    .pending_flavor
                    .clone()
                    .ok_or(StateMachineError::MissingContext("pending_flavor"))?,
            }),
            "add_more" => Ok(Self::AddMore),
            "confirm_address" => Ok(Self::ConfirmAddress),
            "show_summary" => Ok(Self::ShowSummary),
            "wait_receipt" => Ok(Self::WaitReceipt),
            "wait_advisor_response" => Ok(Self::WaitAdvisorResponse),
            "ask_delivery_cost" => Ok(Self::AskDeliveryCost),
            "negotiate_hour" => Ok(Self::NegotiateHour),
            "offer_hour_to_client" => Ok(Self::OfferHourToClient {
                proposed_hour: context
                    .scheduled_time
                    .clone()
                    .unwrap_or_else(|| "pendiente".to_string()),
            }),
            "wait_client_hour" => Ok(Self::WaitClientHour),
            "wait_advisor_hour_decision" => Ok(Self::WaitAdvisorHourDecision {
                client_hour: context
                    .scheduled_time
                    .clone()
                    .unwrap_or_else(|| "pendiente".to_string()),
            }),
            "wait_advisor_confirm_hour" => Ok(Self::WaitAdvisorConfirmHour),
            "wait_advisor_mayor" => Ok(Self::WaitAdvisorMayor),
            "relay_mode" => Ok(Self::RelayMode),
            "contact_advisor_name" => Ok(Self::ContactAdvisorName),
            "contact_advisor_phone" => Ok(Self::ContactAdvisorPhone),
            "wait_advisor_contact" => Ok(Self::WaitAdvisorContact),
            "leave_message" => Ok(Self::LeaveMessage),
            "order_complete" => Ok(Self::OrderComplete),
            _ => Err(StateMachineError::InvalidState(key.to_string())),
        }
    }
}

impl Serialize for ConversationState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_storage_key())
    }
}

impl<'de> Deserialize<'de> for ConversationState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let key = String::deserialize(deserializer)?;
        match key.as_str() {
            "main_menu" => Ok(Self::MainMenu),
            "view_menu" => Ok(Self::ViewMenu),
            "view_schedule" => Ok(Self::ViewSchedule),
            "when_delivery" => Ok(Self::WhenDelivery),
            "check_schedule" => Ok(Self::CheckSchedule),
            "out_of_hours" => Ok(Self::OutOfHours),
            "select_date" => Ok(Self::SelectDate),
            "select_time" => Ok(Self::SelectTime),
            "confirm_schedule" => Ok(Self::ConfirmSchedule),
            "collect_name" => Ok(Self::CollectName),
            "collect_phone" => Ok(Self::CollectPhone),
            "collect_address" => Ok(Self::CollectAddress),
            "select_type" => Ok(Self::SelectType),
            "select_flavor" => Ok(Self::SelectFlavor {
                has_liquor: false,
            }),
            "select_quantity" => Ok(Self::SelectQuantity {
                has_liquor: false,
                flavor: String::new(),
            }),
            "add_more" => Ok(Self::AddMore),
            "confirm_address" => Ok(Self::ConfirmAddress),
            "show_summary" => Ok(Self::ShowSummary),
            "wait_receipt" => Ok(Self::WaitReceipt),
            "wait_advisor_response" => Ok(Self::WaitAdvisorResponse),
            "ask_delivery_cost" => Ok(Self::AskDeliveryCost),
            "negotiate_hour" => Ok(Self::NegotiateHour),
            "offer_hour_to_client" => Ok(Self::OfferHourToClient {
                proposed_hour: String::new(),
            }),
            "wait_client_hour" => Ok(Self::WaitClientHour),
            "wait_advisor_hour_decision" => Ok(Self::WaitAdvisorHourDecision {
                client_hour: String::new(),
            }),
            "wait_advisor_confirm_hour" => Ok(Self::WaitAdvisorConfirmHour),
            "wait_advisor_mayor" => Ok(Self::WaitAdvisorMayor),
            "relay_mode" => Ok(Self::RelayMode),
            "contact_advisor_name" => Ok(Self::ContactAdvisorName),
            "contact_advisor_phone" => Ok(Self::ContactAdvisorPhone),
            "wait_advisor_contact" => Ok(Self::WaitAdvisorContact),
            "leave_message" => Ok(Self::LeaveMessage),
            "order_complete" => Ok(Self::OrderComplete),
            _ => Err(serde::de::Error::custom("invalid conversation state")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserInput {
    ButtonPress(String),
    TextMessage(String),
    ImageMessage(String),
    ListSelection(String),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TimerType {
    AdvisorResponse,
    ReceiptUpload,
    RelayInactivity,
    ConversationAbandon,
}

#[derive(Debug, Clone)]
pub enum BotAction {
    SendText {
        to: String,
        body: String,
    },
    SendButtons {
        to: String,
        body: String,
        buttons: Vec<Button>,
    },
    SendList {
        to: String,
        body: String,
        button_text: String,
        sections: Vec<ListSection>,
    },
    SendImage {
        to: String,
        media_id: String,
        caption: Option<String>,
    },
    StartTimer {
        timer_type: TimerType,
        phone: String,
        duration: std::time::Duration,
    },
    CancelTimer {
        timer_type: TimerType,
        phone: String,
    },
    SaveOrder {
        order: crate::db::models::Order,
    },
    ResetConversation {
        phone: String,
    },
    RelayMessage {
        from: String,
        to: String,
        body: String,
    },
    NoOp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationContext {
    pub phone_number: String,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub delivery_address: Option<String>,
    pub items: Vec<OrderItemData>,
    pub delivery_type: Option<String>,
    pub scheduled_date: Option<String>,
    pub scheduled_time: Option<String>,
    pub payment_method: Option<String>,
    pub receipt_media_id: Option<String>,
    pub pending_has_liquor: Option<bool>,
    pub pending_flavor: Option<String>,
}

impl ConversationContext {
    pub fn from_persisted(
        phone_number: String,
        customer_name: Option<String>,
        customer_phone: Option<String>,
        delivery_address: Option<String>,
        state_data: &ConversationStateData,
    ) -> Self {
        Self {
            phone_number,
            customer_name,
            customer_phone,
            delivery_address,
            items: state_data.items.clone(),
            delivery_type: state_data.delivery_type.clone(),
            scheduled_date: state_data.scheduled_date.clone(),
            scheduled_time: state_data.scheduled_time.clone(),
            payment_method: state_data.payment_method.clone(),
            receipt_media_id: state_data.receipt_media_id.clone(),
            pending_has_liquor: state_data.pending_has_liquor,
            pending_flavor: state_data.pending_flavor.clone(),
        }
    }

    pub fn to_state_data(&self) -> ConversationStateData {
        ConversationStateData {
            items: self.items.clone(),
            delivery_type: self.delivery_type.clone(),
            scheduled_date: self.scheduled_date.clone(),
            scheduled_time: self.scheduled_time.clone(),
            payment_method: self.payment_method.clone(),
            receipt_media_id: self.receipt_media_id.clone(),
            pending_has_liquor: self.pending_has_liquor,
            pending_flavor: self.pending_flavor.clone(),
        }
    }

    pub fn clear_pending_selection(&mut self) {
        self.pending_has_liquor = None;
        self.pending_flavor = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateMachineError {
    InvalidState(String),
    MissingContext(&'static str),
}

impl fmt::Display for StateMachineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState(state) => write!(f, "invalid conversation state: {state}"),
            Self::MissingContext(key) => {
                write!(f, "missing required context to rehydrate state: {key}")
            }
        }
    }
}

impl Error for StateMachineError {}

pub fn transition(
    state: &ConversationState,
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match state {
        ConversationState::MainMenu => menu::handle_main_menu(input, context),
        ConversationState::ViewMenu => menu::handle_view_menu(input, context),
        ConversationState::ViewSchedule => menu::handle_view_schedule(input, context),
        ConversationState::WhenDelivery => scheduling::handle_when_delivery(input, context),
        ConversationState::CheckSchedule => scheduling::handle_check_schedule(context),
        ConversationState::OutOfHours => scheduling::handle_out_of_hours(input, context),
        ConversationState::SelectDate => scheduling::handle_select_date(input, context),
        ConversationState::SelectTime => scheduling::handle_select_time(input, context),
        ConversationState::ConfirmSchedule => scheduling::handle_confirm_schedule(input, context),
        ConversationState::CollectName => data_collect::handle_collect_name(input, context),
        ConversationState::CollectPhone => data_collect::handle_collect_phone(input, context),
        ConversationState::CollectAddress => data_collect::handle_collect_address(input, context),
        ConversationState::SelectType => order::handle_select_type(input, context),
        ConversationState::SelectFlavor { has_liquor } => {
            order::handle_select_flavor(input, context, *has_liquor)
        }
        ConversationState::SelectQuantity { has_liquor, flavor } => {
            order::handle_select_quantity(input, context, *has_liquor, flavor)
        }
        ConversationState::AddMore => order::handle_add_more(input, context),
        ConversationState::ShowSummary => checkout::handle_show_summary(input, context),
        ConversationState::ContactAdvisorName => advisor::handle_contact_advisor_name(input, context),
        ConversationState::ContactAdvisorPhone => {
            advisor::handle_contact_advisor_phone(input, context)
        }
        ConversationState::WaitAdvisorContact => {
            advisor::handle_wait_advisor_contact(input, context)
        }
        ConversationState::LeaveMessage => advisor::handle_leave_message(input, context),
        ConversationState::ConfirmAddress
        | ConversationState::WaitReceipt
        | ConversationState::WaitAdvisorResponse
        | ConversationState::AskDeliveryCost
        | ConversationState::NegotiateHour
        | ConversationState::OfferHourToClient { .. }
        | ConversationState::WaitClientHour
        | ConversationState::WaitAdvisorHourDecision { .. }
        | ConversationState::WaitAdvisorConfirmHour
        | ConversationState::WaitAdvisorMayor => advisor::handle_unimplemented(state, context),
        ConversationState::RelayMode => relay::handle_relay_mode(input, context),
        ConversationState::OrderComplete => checkout::handle_order_complete(context),
    }
}

pub fn extract_input(message: &IncomingMessage) -> UserInput {
    match message.kind.as_str() {
        "text" => UserInput::TextMessage(
            message
                .text
                .as_ref()
                .map(|text| text.body.clone())
                .unwrap_or_default(),
        ),
        "interactive" => match message.interactive.as_ref().map(|item| item.kind.as_str()) {
            Some("button_reply") => UserInput::ButtonPress(
                message
                    .interactive
                    .as_ref()
                    .and_then(|item| item.button_reply.as_ref())
                    .map(|reply| reply.id.clone())
                    .unwrap_or_default(),
            ),
            Some("list_reply") => UserInput::ListSelection(
                message
                    .interactive
                    .as_ref()
                    .and_then(|item| item.list_reply.as_ref())
                    .map(|reply| reply.id.clone())
                    .unwrap_or_default(),
            ),
            _ => UserInput::TextMessage(String::new()),
        },
        "image" => UserInput::ImageMessage(
            message
                .image
                .as_ref()
                .map(|image| image.id.clone())
                .unwrap_or_default(),
        ),
        _ => UserInput::TextMessage(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_input, ConversationContext, ConversationState, UserInput};
    use crate::{
        db::models::{ConversationStateData, OrderItemData},
        whatsapp::types::WebhookPayload,
    };

    fn sample_context() -> ConversationContext {
        ConversationContext {
            phone_number: "573001234567".to_string(),
            customer_name: Some("Ana".to_string()),
            customer_phone: Some("3001234567".to_string()),
            delivery_address: Some("Cra 15 #20-30 Armenia".to_string()),
            items: vec![OrderItemData {
                flavor: "maracuya".to_string(),
                has_liquor: true,
                quantity: 2,
            }],
            delivery_type: Some("scheduled".to_string()),
            scheduled_date: Some("2026-03-11".to_string()),
            scheduled_time: Some("15:30".to_string()),
            payment_method: None,
            receipt_media_id: None,
            pending_has_liquor: Some(true),
            pending_flavor: Some("maracuya".to_string()),
        }
    }

    #[test]
    fn serializes_state_to_snake_case_string() {
        let serialized =
            serde_json::to_string(&ConversationState::WaitAdvisorConfirmHour).expect("serialize");

        assert_eq!(serialized, "\"wait_advisor_confirm_hour\"");
    }

    #[test]
    fn deserializes_state_from_snake_case_string() {
        let state: ConversationState =
            serde_json::from_str("\"show_summary\"").expect("deserialize");

        assert_eq!(state, ConversationState::ShowSummary);
    }

    #[test]
    fn rehydrates_select_quantity_from_storage_and_context() {
        let context = sample_context();
        let state = ConversationState::from_storage_key("select_quantity", &context)
            .expect("rehydrated state");

        assert_eq!(
            state,
            ConversationState::SelectQuantity {
                has_liquor: true,
                flavor: "maracuya".to_string()
            }
        );
    }

    #[test]
    fn context_roundtrip_preserves_pending_fields() {
        let context = sample_context();
        let state_data = context.to_state_data();
        let loaded = ConversationContext::from_persisted(
            context.phone_number.clone(),
            context.customer_name.clone(),
            context.customer_phone.clone(),
            context.delivery_address.clone(),
            &state_data,
        );

        assert_eq!(loaded.pending_has_liquor, Some(true));
        assert_eq!(loaded.pending_flavor.as_deref(), Some("maracuya"));
    }

    #[test]
    fn extracts_text_input() {
        let payload: WebhookPayload = serde_json::from_str(
            r#"{
                "entry": [{
                    "changes": [{
                        "value": {
                            "messages": [{
                                "from": "573001234567",
                                "type": "text",
                                "text": { "body": "Hola" },
                                "id": "wamid.1"
                            }]
                        }
                    }]
                }]
            }"#,
        )
        .expect("payload");

        let input = extract_input(payload.first_message().expect("message"));

        assert_eq!(input, UserInput::TextMessage("Hola".to_string()));
    }

    #[test]
    fn extracts_button_input() {
        let payload: WebhookPayload = serde_json::from_str(
            r#"{
                "entry": [{
                    "changes": [{
                        "value": {
                            "messages": [{
                                "from": "573001234567",
                                "type": "interactive",
                                "interactive": {
                                    "type": "button_reply",
                                    "button_reply": {
                                        "id": "make_order",
                                        "title": "Hacer Pedido"
                                    }
                                },
                                "id": "wamid.2"
                            }]
                        }
                    }]
                }]
            }"#,
        )
        .expect("payload");

        let input = extract_input(payload.first_message().expect("message"));

        assert_eq!(input, UserInput::ButtonPress("make_order".to_string()));
    }

    #[test]
    fn extracts_list_input() {
        let payload: WebhookPayload = serde_json::from_str(
            r#"{
                "entry": [{
                    "changes": [{
                        "value": {
                            "messages": [{
                                "from": "573001234567",
                                "type": "interactive",
                                "interactive": {
                                    "type": "list_reply",
                                    "list_reply": {
                                        "id": "view_schedule",
                                        "title": "Horarios",
                                        "description": "Horarios de entrega"
                                    }
                                },
                                "id": "wamid.3"
                            }]
                        }
                    }]
                }]
            }"#,
        )
        .expect("payload");

        let input = extract_input(payload.first_message().expect("message"));

        assert_eq!(input, UserInput::ListSelection("view_schedule".to_string()));
    }

    #[test]
    fn extracts_image_input() {
        let payload: WebhookPayload = serde_json::from_str(
            r#"{
                "entry": [{
                    "changes": [{
                        "value": {
                            "messages": [{
                                "from": "573001234567",
                                "type": "image",
                                "image": { "id": "media-123" },
                                "id": "wamid.4"
                            }]
                        }
                    }]
                }]
            }"#,
        )
        .expect("payload");

        let input = extract_input(payload.first_message().expect("message"));

        assert_eq!(input, UserInput::ImageMessage("media-123".to_string()));
    }

    #[test]
    fn default_state_data_still_loads_context() {
        let context = ConversationContext::from_persisted(
            "573001234567".to_string(),
            None,
            None,
            None,
            &ConversationStateData::default(),
        );

        assert!(context.items.is_empty());
        assert_eq!(context.pending_has_liquor, None);
    }
}

