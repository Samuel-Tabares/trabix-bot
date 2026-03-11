use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    db::models::{ConversationStateData, OrderItemData},
    whatsapp::types::{Button, IncomingMessage, ListSection},
};

use super::states::{advisor, checkout, data_collect, menu, order, relay, scheduling};

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
                    .advisor_proposed_hour
                    .clone()
                    .unwrap_or_else(|| "pendiente".to_string()),
            }),
            "wait_client_hour" => Ok(Self::WaitClientHour),
            "wait_advisor_hour_decision" => Ok(Self::WaitAdvisorHourDecision {
                client_hour: context
                    .client_counter_hour
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
            "select_flavor" => Ok(Self::SelectFlavor { has_liquor: false }),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageAsset {
    Menu,
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
    SendAssetImage {
        to: String,
        asset: ImageAsset,
        caption: Option<String>,
    },
    SendTransferInstructions {
        to: String,
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
    UpsertDraftOrder {
        status: String,
    },
    FinalizeCurrentOrder {
        status: String,
    },
    UpdateCurrentOrderDeliveryCost {
        delivery_cost: i32,
        total_final: i32,
        status: String,
    },
    CancelCurrentOrder {
        order_id: i32,
    },
    SaveOrder {
        order: crate::db::models::Order,
    },
    BindAdvisorSession {
        advisor_phone: String,
        target_phone: String,
    },
    ClearAdvisorSession {
        advisor_phone: String,
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
    pub advisor_phone: String,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub delivery_address: Option<String>,
    pub items: Vec<OrderItemData>,
    pub delivery_type: Option<String>,
    pub scheduled_date: Option<String>,
    pub scheduled_time: Option<String>,
    pub payment_method: Option<String>,
    pub receipt_media_id: Option<String>,
    pub receipt_timer_started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub advisor_target_phone: Option<String>,
    pub advisor_timer_started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub advisor_timer_expired: bool,
    pub relay_timer_started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub relay_kind: Option<String>,
    pub advisor_proposed_hour: Option<String>,
    pub client_counter_hour: Option<String>,
    pub schedule_resume_target: Option<String>,
    pub current_order_id: Option<i32>,
    pub editing_address: bool,
    pub receipt_timer_expired: bool,
    pub pending_has_liquor: Option<bool>,
    pub pending_flavor: Option<String>,
}

impl ConversationContext {
    pub fn from_persisted(
        phone_number: String,
        advisor_phone: String,
        customer_name: Option<String>,
        customer_phone: Option<String>,
        delivery_address: Option<String>,
        state_data: &ConversationStateData,
    ) -> Self {
        Self {
            phone_number,
            advisor_phone,
            customer_name,
            customer_phone,
            delivery_address,
            items: state_data.items.clone(),
            delivery_type: state_data.delivery_type.clone(),
            scheduled_date: state_data.scheduled_date.clone(),
            scheduled_time: state_data.scheduled_time.clone(),
            payment_method: state_data.payment_method.clone(),
            receipt_media_id: state_data.receipt_media_id.clone(),
            receipt_timer_started_at: state_data.receipt_timer_started_at,
            advisor_target_phone: state_data.advisor_target_phone.clone(),
            advisor_timer_started_at: state_data.advisor_timer_started_at,
            advisor_timer_expired: state_data.advisor_timer_expired,
            relay_timer_started_at: state_data.relay_timer_started_at,
            relay_kind: state_data.relay_kind.clone(),
            advisor_proposed_hour: state_data.advisor_proposed_hour.clone(),
            client_counter_hour: state_data.client_counter_hour.clone(),
            schedule_resume_target: state_data.schedule_resume_target.clone(),
            current_order_id: state_data.current_order_id,
            editing_address: state_data.editing_address,
            receipt_timer_expired: state_data.receipt_timer_expired,
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
            receipt_timer_started_at: self.receipt_timer_started_at,
            advisor_target_phone: self.advisor_target_phone.clone(),
            advisor_timer_started_at: self.advisor_timer_started_at,
            advisor_timer_expired: self.advisor_timer_expired,
            relay_timer_started_at: self.relay_timer_started_at,
            relay_kind: self.relay_kind.clone(),
            advisor_proposed_hour: self.advisor_proposed_hour.clone(),
            client_counter_hour: self.client_counter_hour.clone(),
            schedule_resume_target: self.schedule_resume_target.clone(),
            current_order_id: self.current_order_id,
            editing_address: self.editing_address,
            receipt_timer_expired: self.receipt_timer_expired,
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
        ConversationState::ConfirmAddress => checkout::handle_confirm_address(input, context),
        ConversationState::WaitReceipt => checkout::handle_wait_receipt(input, context),
        ConversationState::WaitAdvisorResponse => {
            checkout::handle_wait_advisor_response(input, context)
        }
        ConversationState::ContactAdvisorName => {
            advisor::handle_contact_advisor_name(input, context)
        }
        ConversationState::ContactAdvisorPhone => {
            advisor::handle_contact_advisor_phone(input, context)
        }
        ConversationState::WaitAdvisorContact => {
            advisor::handle_wait_advisor_contact(input, context)
        }
        ConversationState::LeaveMessage => advisor::handle_leave_message(input, context),
        ConversationState::OfferHourToClient { .. }
        | ConversationState::WaitClientHour
        | ConversationState::AskDeliveryCost
        | ConversationState::NegotiateHour
        | ConversationState::WaitAdvisorHourDecision { .. }
        | ConversationState::WaitAdvisorConfirmHour
        | ConversationState::WaitAdvisorMayor => {
            advisor::handle_client_waiting_state(state, input, context)
        }
        ConversationState::RelayMode => relay::handle_relay_mode(input, context),
        ConversationState::OrderComplete => checkout::handle_order_complete(context),
    }
}

pub fn transition_advisor(
    state: &ConversationState,
    input: &UserInput,
    context: &mut ConversationContext,
) -> TransitionResult {
    match state {
        ConversationState::WaitAdvisorResponse => {
            advisor::handle_advisor_wait_advisor_response(input, context)
        }
        ConversationState::AskDeliveryCost => advisor::handle_advisor_ask_delivery_cost(input, context),
        ConversationState::NegotiateHour => advisor::handle_advisor_negotiate_hour(input, context),
        ConversationState::WaitAdvisorHourDecision { .. } => {
            advisor::handle_advisor_hour_decision(input, context)
        }
        ConversationState::WaitAdvisorConfirmHour => {
            advisor::handle_advisor_confirm_hour(input, context)
        }
        ConversationState::WaitAdvisorMayor => {
            advisor::handle_advisor_wait_advisor_mayor(input, context)
        }
        ConversationState::WaitAdvisorContact => {
            advisor::handle_advisor_wait_advisor_contact(input, context)
        }
        ConversationState::RelayMode => relay::handle_relay_mode_advisor(input, context),
        _ => advisor::handle_advisor_unexpected_state(state, context),
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
            advisor_phone: "573009999999".to_string(),
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
            receipt_timer_started_at: Some(chrono::Utc::now()),
            advisor_target_phone: Some("573001234567".to_string()),
            advisor_timer_started_at: Some(chrono::Utc::now()),
            advisor_timer_expired: false,
            relay_timer_started_at: Some(chrono::Utc::now()),
            relay_kind: Some("wholesale_order".to_string()),
            advisor_proposed_hour: Some("5:00 pm".to_string()),
            client_counter_hour: Some("6:00 pm".to_string()),
            schedule_resume_target: Some("wait_advisor_response".to_string()),
            current_order_id: Some(42),
            editing_address: true,
            receipt_timer_expired: false,
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
            context.advisor_phone.clone(),
            context.customer_name.clone(),
            context.customer_phone.clone(),
            context.delivery_address.clone(),
            &state_data,
        );

        assert_eq!(loaded.pending_has_liquor, Some(true));
        assert_eq!(loaded.pending_flavor.as_deref(), Some("maracuya"));
        assert_eq!(loaded.current_order_id, Some(42));
        assert_eq!(
            loaded.receipt_timer_started_at,
            context.receipt_timer_started_at
        );
        assert!(loaded.editing_address);
        assert_eq!(loaded.advisor_target_phone.as_deref(), Some("573001234567"));
        assert_eq!(loaded.advisor_proposed_hour.as_deref(), Some("5:00 pm"));
        assert_eq!(loaded.client_counter_hour.as_deref(), Some("6:00 pm"));
        assert_eq!(
            loaded.schedule_resume_target.as_deref(),
            Some("wait_advisor_response")
        );
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
            "573009999999".to_string(),
            None,
            None,
            None,
            &ConversationStateData::default(),
        );

        assert!(context.items.is_empty());
        assert_eq!(context.pending_has_liquor, None);
        assert_eq!(context.current_order_id, None);
        assert!(!context.editing_address);
    }
}
