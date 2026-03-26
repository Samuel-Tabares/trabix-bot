use axum::Router;

use crate::{simulator::web, AppState};

pub fn mount(router: Router<AppState>) -> Router<AppState> {
    web::mount(router)
}
